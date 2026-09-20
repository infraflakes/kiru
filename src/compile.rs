use crate::diagnostics::{Diagnostic, Span};
use crate::ir::Program as IrProgram;
use crate::syntax::{FnStmt, Program, Stmt, Template, TopLevel};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};

pub(crate) mod error;

#[cfg(test)]
mod tests;

mod inline;
mod parse;

pub(crate) use error::CompileError;

use inline::{FnResolver, compile_fn_stmts, inline_dsl_template};
use parse::load_import;

/// Run the full compilation pipeline, always building the complete IR (the
/// executor/sync both need the resolved projects).
pub(crate) fn compile_path(entry_path: &Path) -> Result<IrProgram, CompileError> {
    let abs_entry = canonicalize_entry(entry_path)?;
    let mut state = CompileState::new();
    compile_source_file(&abs_entry, &mut state)?;
    Ok(state.arena.build(state.runs))
}

/// Compile an in-memory source string. Used by tests: imports inside `source`
/// still resolve from the filesystem relative to `source_name`, but the root
/// itself is never read back or registered as loaded.
#[cfg(test)]
pub(crate) fn compile_source(
    source_name: &str,
    source_text: &str,
) -> Result<IrProgram, CompileError> {
    let program = parse::parse_source(source_name.to_string(), source_text.to_string())?;
    let mut state = CompileState::new();
    compile_program(&program, &mut state)?;
    Ok(state.arena.build(state.runs))
}

/// One definition of a named function, at the declaration order it appeared.
/// A name may be defined again later; each mention resolves to the newest
/// definition at or before its own declaration point (textual scoping).
struct PendingFn {
    /// Declaration order across every loaded file.
    epoch: usize,
    params: Vec<String>,
    body: Vec<FnStmt>,
    source_name: String,
}

struct CompileState {
    /// Top-level variables, each already inlined to a template with no
    /// `@(var)` references. Commands inside them are preserved as `Cmd`
    /// parts; they are never executed or frozen at compile time.
    globals: BTreeMap<String, Template>,
    /// Named functions accumulated from every source file, keyed by name.
    /// Every definition is kept in declaration order, so a mention resolves
    /// textually: the newest definition at or before the mentioning body's
    /// own declaration point.
    functions: BTreeMap<String, Vec<PendingFn>>,
    /// The arena being filled as declarations are read.
    arena: crate::ir::ProgramBuilder,
    /// Each run's root nodes, lowered the moment its declaration is read.
    runs: BTreeMap<String, Vec<crate::ir::NodeId>>,
    /// Source file text snapshots keyed by source name, used for diagnostic
    /// span rendering in compile errors.
    source_texts: HashMap<String, String>,
    /// Files already compiled in this session, preventing duplicate work
    /// when the same file is imported multiple times.
    loaded_files: HashSet<PathBuf>,
    /// Active import chain for circular-import detection. A file is pushed
    /// before compilation and removed after, so re-entry within the same
    /// chain is an error.
    recursion_stack: HashSet<PathBuf>,
    /// Next declaration order to hand out. Declarations are read top-down,
    /// imports loaded inline, so this is the total textual order.
    next_epoch: usize,
}

impl CompileState {
    fn new() -> Self {
        Self {
            globals: BTreeMap::new(),
            functions: BTreeMap::new(),
            arena: crate::ir::ProgramBuilder::default(),
            runs: BTreeMap::new(),
            source_texts: HashMap::new(),
            loaded_files: HashSet::new(),
            recursion_stack: HashSet::new(),
            next_epoch: 0,
        }
    }

    /// Take the next declaration order.
    fn take_epoch(&mut self) -> usize {
        let epoch = self.next_epoch;
        self.next_epoch += 1;
        epoch
    }

    fn spanned(
        &self,
        msg: impl Into<String>,
        source_name: &str,
        offset: usize,
        len: usize,
    ) -> CompileError {
        error_in(&self.source_texts, source_name, offset, len, msg)
    }
}

/// Build a diagnostic anchored at a span in a source text. The single
/// constructor for the "diagnostic with rendered source" shape; the error
/// wrappers below build on it.
pub(super) fn diagnostic_at(
    source_name: &str,
    source_text: &str,
    span: Span,
    msg: impl Into<String>,
) -> Diagnostic {
    Diagnostic::new(
        source_name.to_string(),
        span,
        msg.into(),
        source_text.to_string(),
    )
}

/// Build a compile error anchored at a span in a source text.
pub(super) fn error_at(
    source_name: &str,
    source_text: &str,
    span: Span,
    msg: impl Into<String>,
) -> CompileError {
    CompileError::diagnostic(diagnostic_at(source_name, source_text, span, msg))
}

/// Build a diagnostic anchored at a span in a registered source, looked up
/// by name in the `source_texts` map. The lookup falls back to an empty
/// text so the constructor stays total; a missing entry cannot happen for
/// parsed sources.
pub(super) fn diagnostic_in(
    sources: &HashMap<String, String>,
    source_name: &str,
    offset: usize,
    len: usize,
    msg: impl Into<String>,
) -> Diagnostic {
    let source_text = sources.get(source_name).cloned().unwrap_or_default();
    diagnostic_at(source_name, &source_text, Span::new(offset, len), msg)
}

/// Build a compile error anchored at a span in a registered source.
pub(super) fn error_in(
    sources: &HashMap<String, String>,
    source_name: &str,
    offset: usize,
    len: usize,
    msg: impl Into<String>,
) -> CompileError {
    CompileError::diagnostic(diagnostic_in(sources, source_name, offset, len, msg))
}

/// Resolve a path to an absolute, canonical location.
pub(crate) fn canonicalize_entry(path: &Path) -> Result<PathBuf, CompileError> {
    let abs_path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(CompileError::Io)?
            .join(path)
    };
    std::fs::canonicalize(&abs_path).map_err(|e| {
        CompileError::Io(std::io::Error::new(
            e.kind(),
            format!("failed to resolve {}: {}", abs_path.display(), e),
        ))
    })
}

fn compile_source_file(file_path: &Path, state: &mut CompileState) -> Result<(), CompileError> {
    let canon_path = std::fs::canonicalize(file_path).map_err(|e| {
        CompileError::Io(std::io::Error::new(
            e.kind(),
            format!("failed to resolve {}: {}", file_path.display(), e),
        ))
    })?;
    if state.recursion_stack.contains(&canon_path) {
        return Err(state.spanned(
            format!("circular import: {}", canon_path.display()),
            &canon_path.display().to_string(),
            0,
            1,
        ));
    }
    if state.loaded_files.contains(&canon_path) {
        return Ok(());
    }
    state.recursion_stack.insert(canon_path.clone());
    let program = parse::parse_file(&canon_path)?;
    let result = compile_program(&program, state);
    state.recursion_stack.remove(&canon_path);
    result
}

fn compile_program(program: &Program, state: &mut CompileState) -> Result<(), CompileError> {
    state
        .source_texts
        .insert(program.source_name.clone(), program.source_text.clone());
    // Everything is read top-down: a declaration joins the namespace at the
    // point it appears, and imports are loaded inline, so later declarations
    // in later files are simply later. A mention can only resolve against
    // what was declared before it.
    for item in &program.top_level_items {
        match item {
            TopLevel::Stmt(stmt) => compile_stmt(stmt, state, program)?,
            TopLevel::Import(path) => {
                load_import(path, state, program)?;
            }
        }
    }
    Ok(())
}

fn compile_stmt(
    stmt: &Stmt,
    state: &mut CompileState,
    program: &Program,
) -> Result<(), CompileError> {
    match stmt {
        Stmt::Var { name, value } => compile_var_decl(name, value, &program.source_name, state),
        Stmt::Fn {
            name, params, body, ..
        } => {
            // Register this definition at its point in the text, then
            // validate the body right here, so a mention of a name declared
            // later (or an undefined name) errors even if the function is
            // never called.
            let epoch = state.take_epoch();
            state
                .functions
                .entry(name.clone())
                .or_default()
                .push(PendingFn {
                    epoch,
                    params: params.clone(),
                    body: body.clone(),
                    source_name: program.source_name.clone(),
                });
            validate_function_body(state, name, params, body, epoch, &program.source_name)
        }
        Stmt::Run { name, body } => {
            // Runs are entry points: lower immediately against the
            // declarations visible here.
            let bound = state.next_epoch;
            let mut scope = state.globals.clone();
            let mut cycle_stack = Vec::new();
            let resolver = FnResolver {
                functions: &state.functions,
            };
            let children = compile_fn_stmts(
                body,
                &mut scope,
                &state.source_texts,
                &program.source_name,
                &resolver,
                &mut cycle_stack,
                bound,
                &mut state.arena,
            )?;
            state.runs.insert(name.clone(), children);
            Ok(())
        }
    }
}

/// Validate one function body at its declaration point: lower it once with
/// each parameter bound to an empty placeholder, against the definitions
/// visible up to `epoch`, and discard the result. Mistakes in dead code
/// (undefined names, mentions of later declarations, unusable case
/// patterns) are still compile errors. Call sites re-lower the body with
/// the real arguments, which stays the authority on argument-dependent
/// checks.
fn validate_function_body(
    state: &CompileState,
    name: &str,
    params: &[String],
    body: &[FnStmt],
    epoch: usize,
    source_name: &str,
) -> Result<(), CompileError> {
    let mut scope = BTreeMap::new();
    for param in params {
        scope.insert(param.clone(), Template::default());
    }
    let resolver = FnResolver {
        functions: &state.functions,
    };
    let mut cycle_stack = vec![name.to_string()];
    let mut scratch = crate::ir::ProgramBuilder::default();
    compile_fn_stmts(
        body,
        &mut scope,
        &state.source_texts,
        source_name,
        &resolver,
        &mut cycle_stack,
        epoch,
        &mut scratch,
    )?;
    Ok(())
}

/// Declare a top-level variable: inline its template against the variables
/// declared before it and record it for later inlining. A later declaration
/// of the same name simply replaces it from that point on.
fn compile_var_decl(
    name: &str,
    value: &Template,
    source_name: &str,
    state: &mut CompileState,
) -> Result<(), CompileError> {
    let inlined = inline_dsl_template(value, &state.globals, &state.source_texts, source_name)?;
    state.globals.insert(name.to_string(), inlined);
    Ok(())
}
