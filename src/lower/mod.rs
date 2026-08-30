use crate::diagnostics::{Diagnostic, Span};
use crate::ir::{Call, Instruction, Ir};
use crate::syntax::{Program, ProjectField, Stmt, Template, TopLevel};
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};

pub(crate) mod error;
#[cfg(test)]
pub(crate) mod test_support;

#[cfg(test)]
mod tests;

mod build;
mod inline;
mod parse;

pub use error::CompileError;

use build::build_ir;
use inline::{inline_dsl_template, lower_function_body};
use parse::{load_import, render_literal};

/// Run the full compilation pipeline, always building the complete IR (the
/// executor/sync both need the resolved projects).
pub fn lower_and_resolve(entry_path: &Path, _force_cwd: bool) -> Result<Ir, CompileError> {
    let abs_entry = canonicalize_entry(entry_path)?;
    let mut state = LoweringState::new();
    compile_source_file(&abs_entry, &mut state)?;
    build_ir(state)
}

struct LoweringState {
    /// Static variables (top-level and `pr`-body), each already inlined to a
    /// template with no `@(var)` references. Commands inside them are preserved
    /// as `Cmd` parts — they are never executed or frozen at compile time.
    globals: BTreeMap<String, Template>,
    shell: Option<String>,
    syncs: BTreeMap<String, PendingSync>,
    projects: BTreeMap<String, PendingProject>,
    run_blocks: BTreeMap<String, Vec<Vec<Call>>>,
    source_texts: HashMap<String, String>,
    loaded_files: HashSet<PathBuf>,
    recursion_stack: HashSet<PathBuf>,
}

/// A repository/sync block being accumulated (fields only).
struct PendingSync {
    url: Option<Template>,
    dir: Option<Template>,
    branch: Option<Template>,
    strategy: Option<Template>,
}

/// A project block being accumulated: inlined static vars and lowered function
/// bodies. Function-local `bind` variables are inlined away during lowering, so
/// nothing static survives here either.
struct PendingProject {
    vars: BTreeMap<String, Template>,
    functions: BTreeMap<String, Vec<Instruction>>,
}

impl LoweringState {
    fn new() -> Self {
        Self {
            globals: BTreeMap::new(),
            shell: None,
            syncs: BTreeMap::new(),
            projects: BTreeMap::new(),
            run_blocks: BTreeMap::new(),
            source_texts: HashMap::new(),
            loaded_files: HashSet::new(),
            recursion_stack: HashSet::new(),
        }
    }

    fn shell(&self) -> String {
        self.shell.clone().unwrap_or_else(|| "sh".to_string())
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
        CompileError::Validation(vec![Diagnostic::new(
            source_name.to_string(),
            Span::new(offset, len),
            msg,
            src,
        )])
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

fn compile_source_file(file_path: &Path, state: &mut LoweringState) -> Result<(), CompileError> {
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

fn compile_program(program: &Program, state: &mut LoweringState) -> Result<(), CompileError> {
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
    state: &mut LoweringState,
    program: &Program,
) -> Result<(), CompileError> {
    match stmt {
        Stmt::Shell {
            value,
            offset,
            len,
            source_name,
        } => {
            let inlined =
                inline_dsl_template(value, &state.globals, &state.source_texts, source_name)?;
            let resolved = render_literal(&inlined);
            if state.shell.is_some() {
                return Err(state.spanned(
                    "duplicate shell declaration".to_string(),
                    source_name,
                    *offset,
                    *len,
                ));
            }
            state.shell = Some(resolved);
            Ok(())
        }
        Stmt::Var {
            name,
            value,
            offset,
            len,
        } => {
            let inlined = inline_dsl_template(
                value,
                &state.globals,
                &state.source_texts,
                &program.source_name,
            )?;
            if state.globals.contains_key(name) {
                return Err(state.spanned(
                    format!("variable `{}` is already defined", name),
                    &program.source_name,
                    *offset,
                    *len,
                ));
            }
            state.globals.insert(name.clone(), inlined);
            Ok(())
        }
        Stmt::Fn {
            name,
            body,
            offset,
            len,
        } => {
            // Top-level function: attached to no project. Kept so it can be
            // referenced, but `kiru` runs functions inside `pr` blocks.
            let _ = (name, body, offset, len);
            Ok(())
        }
        Stmt::Project { name, fields, body } => compile_project(name, fields, body, state, program),
        Stmt::Run {
            name,
            calls,
            offset,
            len,
        } => {
            if state.run_blocks.contains_key(name) {
                return Err(state.spanned(
                    format!("duplicate run block: {}", name),
                    &program.source_name,
                    *offset,
                    *len,
                ));
            }
            // Convert from syntax::ast::Call to ir::Call
            let ir_calls: Vec<Vec<Call>> = calls
                .iter()
                .map(|chain| {
                    chain
                        .iter()
                        .map(|c| Call {
                            project: c.project.clone(),
                            function: c.function.clone(),
                        })
                        .collect()
                })
                .collect();
            state.run_blocks.insert(name.clone(), ir_calls);
            Ok(())
        }
        Stmt::Field { .. } => Ok(()),
    }
}

fn compile_project(
    name: &str,
    fields: &[Stmt],
    body: &[Stmt],
    state: &mut LoweringState,
    program: &Program,
) -> Result<(), CompileError> {
    // Sync fields (if any) accumulate into the syncs map.
    if !fields.is_empty() {
        let pending = state
            .syncs
            .entry(name.to_string())
            .or_insert_with(|| PendingSync {
                url: None,
                dir: None,
                branch: None,
                strategy: None,
            });
        for field in fields {
            if let Stmt::Field {
                key,
                value,
                offset,
                len,
            } = field
            {
                let resolved = inline_dsl_template(
                    value,
                    &state.globals,
                    &state.source_texts,
                    &program.source_name,
                )?;
                match key {
                    ProjectField::Url => {
                        if pending.url.is_some() {
                            return Err(state.spanned(
                                "duplicate field 'url'".to_string(),
                                &program.source_name,
                                *offset,
                                *len,
                            ));
                        }
                        pending.url = Some(resolved);
                    }
                    ProjectField::Dir => {
                        if pending.dir.is_some() {
                            return Err(state.spanned(
                                "duplicate field 'dir'".to_string(),
                                &program.source_name,
                                *offset,
                                *len,
                            ));
                        }
                        pending.dir = Some(resolved);
                    }
                    ProjectField::Branch => {
                        if pending.branch.is_some() {
                            return Err(state.spanned(
                                "duplicate field 'branch'".to_string(),
                                &program.source_name,
                                *offset,
                                *len,
                            ));
                        }
                        pending.branch = Some(resolved);
                    }
                    ProjectField::Sync => {
                        if pending.strategy.is_some() {
                            return Err(state.spanned(
                                "duplicate field 'sync'".to_string(),
                                &program.source_name,
                                *offset,
                                *len,
                            ));
                        }
                        pending.strategy = Some(resolved);
                    }
                }
            }
        }
    }

    // Project body: `var` (frozen), `fn` (lowered to instructions).
    let pending = state
        .projects
        .entry(name.to_string())
        .or_insert_with(|| PendingProject {
            vars: BTreeMap::new(),
            functions: BTreeMap::new(),
        });

    // Scope for resolving this project's vars: globals + already-defined vars.
    let mut scope = state.globals.clone();
    for (k, v) in &pending.vars {
        scope.insert(k.clone(), v.clone());
    }

    for stmt in body {
        match stmt {
            Stmt::Var {
                name: var_name,
                value,
                offset,
                len,
            } => {
                let resolved =
                    inline_dsl_template(value, &scope, &state.source_texts, &program.source_name)?;
                if pending.vars.contains_key(var_name) {
                    return Err(state.spanned(
                        format!(
                            "variable `{}` is already defined in project `{}`",
                            var_name, name
                        ),
                        &program.source_name,
                        *offset,
                        *len,
                    ));
                }
                pending.vars.insert(var_name.clone(), resolved.clone());
                scope.insert(var_name.clone(), resolved);
            }
            Stmt::Fn {
                name: fn_name,
                body: fn_body,
                offset,
                len,
            } => {
                if pending.functions.contains_key(fn_name) {
                    return Err(state.spanned(
                        format!("duplicate function `{}` in project `{}`", fn_name, name),
                        &program.source_name,
                        *offset,
                        *len,
                    ));
                }
                let lowered = lower_function_body(
                    fn_body,
                    &scope,
                    &state.source_texts,
                    &program.source_name,
                )?;
                pending.functions.insert(fn_name.clone(), lowered);
            }
            _ => {}
        }
    }

    Ok(())
}
