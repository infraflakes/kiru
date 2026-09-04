use crate::diagnostics::{Diagnostic, Span};
use crate::ir::{Call, Instruction, Ir};
use crate::syntax::{Program, Stmt, Template, TopLevel};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};

pub(crate) mod error;
#[cfg(test)]
mod test_support;

#[cfg(test)]
mod tests;

mod build;
mod inline;
mod parse;
mod stmt;

pub(crate) use error::CompileError;

use build::build_ir;
use parse::load_import;

/// Run the full compilation pipeline, always building the complete IR (the
/// executor/sync both need the resolved projects).
pub(crate) fn compile_path(entry_path: &Path) -> Result<Ir, CompileError> {
    let abs_entry = canonicalize_entry(entry_path)?;
    let mut state = CompileState::new();
    compile_source_file(&abs_entry, &mut state)?;
    build_ir(state)
}

/// Compile an in-memory source string. Used by tests: imports inside `source`
/// still resolve from the filesystem relative to `source_name`, but the root
/// itself is never read back or registered as loaded.
#[cfg(test)]
pub(crate) fn compile_source(source_name: &str, source_text: &str) -> Result<Ir, CompileError> {
    let program = parse::parse_source(source_name.to_string(), source_text.to_string())?;
    let mut state = CompileState::new();
    compile_program(&program, &mut state)?;
    build_ir(state)
}

/// A run block accumulated from `run name { ... }` syntax: the ordered
/// stages of project-function calls plus the declaration's source location,
/// so reference validation can report errors against the real span.
struct PendingRunBlock {
    stages: Vec<Vec<Call>>,
    source_name: String,
    offset: usize,
    len: usize,
}

struct CompileState {
    /// Static variables (top-level and `pr`-body), each already inlined to a
    /// template with no `@(var)` references. Commands inside them are preserved
    /// as `Cmd` parts, they are never executed or frozen at compile time.
    globals: BTreeMap<String, Template>,
    /// Project blocks accumulated from `pr name { ... }` syntax, each
    /// containing inlined static vars and compiled function bodies.
    projects: BTreeMap<String, PendingProject>,
    /// Run blocks accumulated from `run name { ... }` syntax, each being
    /// an ordered list of sequential chains of project-function calls.
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

/// A project block being accumulated: inlined static vars and lowered function
/// bodies. Function-local `bind` variables are inlined away during compilation, so
/// nothing static survives here either.
struct PendingProject {
    vars: BTreeMap<String, Template>,
    functions: BTreeMap<String, Vec<Instruction>>,
}

impl CompileState {
    fn new() -> Self {
        Self {
            globals: BTreeMap::new(),
            projects: BTreeMap::new(),
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
        let src = self
            .source_texts
            .get(source_name)
            .cloned()
            .unwrap_or_default();
        CompileError::diagnostic(Diagnostic::new(
            source_name.to_string(),
            Span::new(offset, len),
            msg,
            src,
        ))
    }
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
        } => stmt::compile_var_decl(name, value, *offset, *len, &program.source_name, state),
        Stmt::Fn { .. } => Ok(()),
        Stmt::Project { name, body } => {
            stmt::compile_project_body(name, body, &program.source_name, state)
        }
        Stmt::Run {
            name,
            calls,
            offset,
            len,
        } => stmt::compile_run_decl(name, calls, *offset, *len, &program.source_name, state),
    }
}
