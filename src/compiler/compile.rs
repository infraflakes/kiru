use crate::compiler::error::CompileError;
use crate::compiler::error::spanned_err;

use crate::compiler::resolve;
use crate::compiler::scope::{ScopeKind, ScopeStack};
use crate::compiler::types::{Config, Project, UnresolvedProject};
use crate::compiler::validation;
use crate::dsl::Parser;
use crate::dsl::{Expr, Program, Stmt, TopLevel};
use miette::miette;
use std::collections::HashMap;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// Run the full compilation pipeline:
/// 1. Linear processing: walk items in source order, resolve vars and fields,
///    load imports with variable interpolation, accumulate projects.
/// 2. Validate using the resolved state.
/// 3. Fully resolve function bodies against the flat var scope.
pub fn compile_and_resolve(entry_path: &Path) -> Result<Config, CompileError> {
    let abs_entry = canonicalize_entry(entry_path)?;
    let linear_result = resolve_linear(&abs_entry)?;
    validation::validate_configuration(&linear_result.unresolved, &linear_result.var_scope)?;
    resolve::resolve_with_scopes(linear_result.unresolved, linear_result.var_scope)
}

// Linear-processing pipeline

/// Mutable state threaded through the linear processing phase.
struct LinearState {
    var_scope: ScopeStack<String>,
    projects: HashMap<String, UnresolvedProject>,
    loaded_files: HashSet<PathBuf>,
    recursion_stack: HashSet<PathBuf>,
}

impl LinearState {
    fn new() -> Self {
        Self {
            var_scope: ScopeStack::new(),
            projects: HashMap::new(),
            loaded_files: HashSet::new(),
            recursion_stack: HashSet::new(),
        }
    }
}

/// Intermediate result from the linear processing phase.
struct LinearResult {
    unresolved: super::types::UnresolvedConfig,
    var_scope: ScopeStack<String>,
}

/// Canonicalize the entry path, resolving relative paths against the current
/// working directory. All paths within the pipeline are absolute after this.
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
            format!(
                "Failed to resolve {} (from {}): {}",
                abs_path.display(),
                path.display(),
                e
            ),
        ))
    })
}

/// Parse a single file into a [`Program`]. `canon_path` MUST be the canonical
/// absolute path. Does NOT manage `recursion_stack` — the caller is responsible
/// for that. Marks the file as loaded on success to prevent re-parsing.
fn parse_file(
    canon_path: &Path,
    loaded_files: &mut HashSet<PathBuf>,
) -> Result<Program, CompileError> {
    if loaded_files.contains(canon_path) {
        return Ok(Program::new());
    }

    let data = std::fs::read_to_string(canon_path).map_err(|e| {
        CompileError::Io(std::io::Error::new(
            e.kind(),
            format!("Failed to read {}: {}", canon_path.display(), e),
        ))
    })?;

    let source_name = canon_path.display().to_string();
    let source_text = data.clone();
    let mut parser = Parser::from_source(data);
    let mut program = Program::new();
    program.set_source(source_name, source_text);

    while let Some(toplevel) = parser.parse_toplevel().map_err(|e| {
        CompileError::ParseReports(vec![miette::Report::new(e).with_source_code(
            miette::NamedSource::new(program.source_name.clone(), program.source_text.clone()),
        )])
    })? {
        program.items.push(toplevel);
    }

    loaded_files.insert(canon_path.to_path_buf());
    Ok(program)
}

/// Walk items in lexical order, resolving vars into scopes, loading imports
/// when their paths become resolvable, and accumulating projects.
fn linear_process_file(file_path: &Path, state: &mut LinearState) -> Result<(), CompileError> {
    let canon_path = std::fs::canonicalize(file_path).map_err(|e| {
        CompileError::Io(std::io::Error::new(
            e.kind(),
            format!("Failed to resolve {}: {}", file_path.display(), e),
        ))
    })?;

    if state.recursion_stack.contains(&canon_path) {
        return Err(CompileError::ValidationReport(miette!(
            "circular import: {}",
            canon_path.display()
        )));
    }

    if state.loaded_files.contains(&canon_path) {
        return Ok(());
    }

    state.recursion_stack.insert(canon_path.clone());
    let program = parse_file(&canon_path, &mut state.loaded_files)?;

    let result = linear_process_program(&program, state);

    state.recursion_stack.remove(&canon_path);
    result
}

use crate::dsl::ProjectField;

/// Merge a single statement into a project body during AST collection.
pub(crate) fn merge_project_body_stmt(
    project: &mut UnresolvedProject,
    stmt: &Stmt,
    source_name: &str,
    source_text: &str,
) -> Result<(), CompileError> {
    let make_err = |msg: String, offset: usize, len: usize| -> CompileError {
        spanned_err(msg, source_name, source_text, offset, len)
    };
    match stmt {
        Stmt::Var { .. } => {}
        Stmt::Field {
            key,
            value,
            offset,
            len,
            ..
        } => {
            let already_set = match key {
                ProjectField::Url => project.url.is_some(),
                ProjectField::Dir => project.dir.is_some(),
                ProjectField::Sync => project.sync.is_some(),
                ProjectField::Branch => project.branch.is_some(),
            };
            if already_set {
                return Err(make_err(
                    format!("duplicate field '{:?}' in project '{}'", key, project.name),
                    *offset,
                    *len,
                ));
            }
            match key {
                ProjectField::Url => project.url = Some(value.clone()),
                ProjectField::Dir => project.dir = Some(value.clone()),
                ProjectField::Sync => project.sync = Some(value.clone()),
                ProjectField::Branch => project.branch = Some(value.clone()),
            }
        }
        Stmt::Fn {
            name,
            body,
            offset,
            len,
            ..
        } => {
            if project.functions.contains_key(name) {
                return Err(make_err(
                    format!("duplicate function in project '{}': {}", project.name, name),
                    *offset,
                    *len,
                ));
            }
            project.functions.insert(name.clone(), body.clone());
        }
        Stmt::Run {
            name,
            chains,
            offset,
            len,
            ..
        } => {
            if project.runs.contains_key(name) {
                return Err(make_err(
                    format!(
                        "duplicate run block in project '{}': {}",
                        project.name, name
                    ),
                    *offset,
                    *len,
                ));
            }
            project.runs.insert(name.clone(), chains.clone());
        }
        Stmt::Project { offset, len, .. } => {
            return Err(spanned_err(
                format!(
                    "unexpected statement in project '{}' (only var, fn, and run are valid)",
                    project.name
                ),
                source_name,
                source_text,
                *offset,
                *len,
            ));
        }
    }
    Ok(())
}

/// Process a `pr <name> { ... }` block: set up the project entry, populate
/// fields, resolve var stmts into a Project frame, and collect fn/run blocks.
fn process_project_block(
    name: &str,
    fields: &[Stmt],
    body: &[Stmt],
    state: &mut LinearState,
    program: &Program,
) -> Result<(), CompileError> {
    let project_entry =
        state
            .projects
            .entry(name.to_owned())
            .or_insert_with(|| UnresolvedProject {
                name: name.to_owned(),
                source_file: program.source_name.clone(),
                source_text: program.source_text.clone(),
                url: None,
                dir: None,
                sync: None,
                branch: None,
                vars: HashMap::new(),
                functions: HashMap::new(),
                runs: HashMap::new(),
            });

    for field_stmt in fields {
        merge_project_body_stmt(
            project_entry,
            field_stmt,
            &program.source_name,
            &program.source_text,
        )?;
    }

    // Push a Project frame so project body vars go into it and duplicate
    // detection (via ScopeStack::declare) checks global + project chain.
    state.var_scope.push_frame(ScopeKind::Project);
    for body_stmt in body {
        if let Stmt::Var { .. } = body_stmt {
            resolve::resolve_var_stmt(
                body_stmt,
                &mut state.var_scope,
                &program.source_name,
                &program.source_text,
            )?;
        }
        merge_project_body_stmt(
            project_entry,
            body_stmt,
            &program.source_name,
            &program.source_text,
        )?;
    }
    let entries = state.var_scope.pop_frame_entries();
    project_entry.vars = entries.into_iter().collect();
    Ok(())
}

/// Resolve and load an `import` statement. The expression is first
/// interpolated (variable substitution), then resolved against the
/// directory of the source file.  `Path::join` handles absolute vs
/// relative transparently.
fn process_import(
    expr: &Expr,
    state: &mut LinearState,
    program: &Program,
) -> Result<(), CompileError> {
    let path_str = resolve::resolve_expr(
        expr,
        &state.var_scope,
        &program.source_name,
        &program.source_text,
    )?;
    if path_str.is_empty() {
        let (offset, len) = expr.offset_len();
        return Err(spanned_err(
            "import path cannot be empty".to_string(),
            &program.source_name,
            &program.source_text,
            offset,
            len,
        ));
    }
    let base_dir = Path::new(&program.source_name).parent().ok_or_else(|| {
        CompileError::ValidationReport(miette!(
            "cannot determine base directory for import from '{}'",
            program.source_name
        ))
    })?;
    linear_process_file(&base_dir.join(&path_str), state)
}

/// Process a program's items in lexical order.
fn linear_process_program(program: &Program, state: &mut LinearState) -> Result<(), CompileError> {
    for item in &program.items {
        match item {
            TopLevel::Stmt(stmt) => match stmt {
                Stmt::Var { .. } => {
                    resolve::resolve_var_stmt(
                        stmt,
                        &mut state.var_scope,
                        &program.source_name,
                        &program.source_text,
                    )?;
                }
                Stmt::Project {
                    name, fields, body, ..
                } => {
                    process_project_block(name, fields, body, state, program)?;
                }
                Stmt::Fn { offset, len, .. } | Stmt::Run { offset, len, .. } => {
                    return Err(spanned_err(
                        format!("unexpected statement in '{}'", program.source_name),
                        &program.source_name,
                        &program.source_text,
                        *offset,
                        *len,
                    ));
                }
                Stmt::Field {
                    key, offset, len, ..
                } => {
                    return Err(spanned_err(
                        format!(
                            "field '{:?}' is not inside a project block in '{}'",
                            key, program.source_name
                        ),
                        &program.source_name,
                        &program.source_text,
                        *offset,
                        *len,
                    ));
                }
            },
            TopLevel::Import(expr) => process_import(expr, state, program)?,
        }
    }
    Ok(())
}

/// The core linear processing phase: entry point.
fn resolve_linear(entry_path: &Path) -> Result<LinearResult, CompileError> {
    let mut state = LinearState::new();
    linear_process_file(entry_path, &mut state)?;

    let unresolved = super::types::UnresolvedConfig {
        projects: state.projects,
    };

    Ok(LinearResult {
        unresolved,
        var_scope: state.var_scope,
    })
}

/// Lightweight compilation that resolves project metadata without validating
/// or lowering function bodies.
///
/// 1. Linear processing — parse the entry file, resolve `var` and `var shell`
///    declarations, build the flat var scope, follow imports.
/// 2. Project field resolution — resolve each project's `url`, `dir`, `sync`,
///    and `branch` expressions against the flat scope.
///
/// The returned [`Config`] has empty function maps — function and run
/// blocks are collected during linear processing but never resolved.
pub fn parse_projects_metadata(entry_path: &Path) -> Result<Config, CompileError> {
    let abs_entry = canonicalize_entry(entry_path)?;
    let linear = resolve_linear(&abs_entry)?;

    let mut projects = HashMap::new();
    for (name, unresolved_project) in linear.unresolved.projects {
        // Build a combined scope (global + project vars) for field resolution.
        let mut scope = ScopeStack::new();
        scope.seed_global(
            linear
                .var_scope
                .iter_global()
                .map(|(k, v)| (k.clone(), v.clone())),
        );
        scope.push_frame(ScopeKind::Project);
        scope.seed_top(unresolved_project.vars.clone());

        let (url, dir, sync, branch) =
            resolve::resolve_project_fields(&unresolved_project, &scope)?;

        projects.insert(
            name,
            Project {
                url,
                dir,
                sync,
                branch,
                functions: HashMap::new(),
                runs: unresolved_project.runs,
            },
        );
    }

    Ok(Config { projects })
}
