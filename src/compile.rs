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
    build_program(state)
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
    build_program(state)
}

/// A run block accumulated from `run name { ... }` syntax: its unresolved
/// body plus the source location, so reference and arity errors report
/// against the real span. The body is lowered in `build_program`, once
/// every function is collected.
struct PendingRunBlock {
    body: Vec<FnStmt>,
    source_name: String,
}

/// A named function awaiting lowering: its parameters, body, and the source
/// those statements were written in (possibly an imported file). Bodies are
/// lowered per call site, with arguments bound to params, so this map is
/// never lowered on its own.
struct PendingFn {
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
    /// Expansions happen per call site, so this map is never lowered on its
    /// own.
    functions: BTreeMap<String, PendingFn>,
    /// Run blocks accumulated from `run name { ... }` syntax, each being
    /// an ordered list of sequential chains of function calls.
    run_blocks: BTreeMap<String, PendingRunBlock>,
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
}

impl CompileState {
    fn new() -> Self {
        Self {
            globals: BTreeMap::new(),
            functions: BTreeMap::new(),
            run_blocks: BTreeMap::new(),
            source_texts: HashMap::new(),
            loaded_files: HashSet::new(),
            recursion_stack: HashSet::new(),
        }
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
    // Pre-pass: collect every top-level `fn`, wherever it appears in the
    // file, so any function can call any other regardless of order. Imports
    // keep their file-order rule: functions from an import are only visible
    // after its import statement, matching everything else from that file.
    for item in &program.top_level_items {
        if let TopLevel::Stmt(Stmt::Fn {
            name,
            params,
            body,
            offset,
            len,
        }) = item
        {
            if state.functions.contains_key(name) {
                return Err(state.spanned(
                    format!("duplicate function `{name}`"),
                    &program.source_name,
                    *offset,
                    *len,
                ));
            }
            state.functions.insert(
                name.clone(),
                PendingFn {
                    params: params.clone(),
                    body: body.clone(),
                    source_name: program.source_name.clone(),
                },
            );
        }
    }
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
        Stmt::Var {
            name,
            value,
            offset,
            len,
        } => compile_var_decl(name, value, *offset, *len, &program.source_name, state),
        // Function bodies were collected by the pre-pass and are lowered per
        // call site during `build_program`.
        Stmt::Fn { .. } => Ok(()),
        Stmt::Run {
            name,
            body,
            offset,
            len,
        } => compile_run_decl(name, body, *offset, *len, &program.source_name, state),
    }
}

/// Lower every run body into nodes and freeze the arena. Run bodies are no
/// different from function bodies: calls are flattened and blocks own their
/// children, so the display tree and the execution tree are one structure.
fn build_program(state: CompileState) -> Result<IrProgram, CompileError> {
    let CompileState {
        globals,
        functions,
        run_blocks,
        source_texts,
        loaded_files: _,
        recursion_stack: _,
    } = state;

    let resolver = FnResolver {
        globals: &globals,
        functions: &functions,
    };

    let mut runs = BTreeMap::new();
    let mut arena = crate::ir::ProgramBuilder::default();
    for (run_name, pending_run) in run_blocks {
        let PendingRunBlock { body, source_name } = pending_run;
        let mut scope = globals.clone();
        let mut cycle_stack = Vec::new();
        let children = compile_fn_stmts(
            &body,
            &mut scope,
            &source_texts,
            &source_name,
            &resolver,
            &mut cycle_stack,
            &mut arena,
        )?;
        runs.insert(run_name, children);
    }

    Ok(arena.build(runs))
}

/// Declare a top-level variable: inline its template against the globals and
/// record it for later inlining at every use site.
fn compile_var_decl(
    name: &str,
    value: &Template,
    offset: usize,
    len: usize,
    source_name: &str,
    state: &mut CompileState,
) -> Result<(), CompileError> {
    let inlined = inline_dsl_template(value, &state.globals, &state.source_texts, source_name)?;
    if state.globals.contains_key(name) {
        return Err(state.spanned(
            format!("variable `{}` is already defined", name),
            source_name,
            offset,
            len,
        ));
    }
    state.globals.insert(name.to_string(), inlined);
    Ok(())
}

/// Accumulate a `run name { ... }` block; bodies are lowered once every
/// function is collected, so a run may call functions declared later.
fn compile_run_decl(
    name: &str,
    body: &[FnStmt],
    offset: usize,
    len: usize,
    source_name: &str,
    state: &mut CompileState,
) -> Result<(), CompileError> {
    if state.run_blocks.contains_key(name) {
        return Err(state.spanned(
            format!("duplicate run block: {}", name),
            source_name,
            offset,
            len,
        ));
    }
    state.run_blocks.insert(
        name.to_string(),
        PendingRunBlock {
            body: body.to_vec(),
            source_name: source_name.to_string(),
        },
    );
    Ok(())
}
