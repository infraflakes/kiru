use crate::compiler::error::CompileError;
use crate::compiler::error::spanned_err;
use crate::compiler::imports;
use crate::compiler::resolve;
use crate::compiler::types::{Config, Project, SyncMode, UnresolvedProject};
use crate::compiler::validation;
use crate::dsl::Parser;
use crate::dsl::{Program, Stmt, TopLevel};
use miette::miette;
use std::collections::HashMap;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// Run the full compilation pipeline:
/// 1. Linear processing: walk items in source order, resolve vars and fields,
///    load imports with variable interpolation, accumulate projects.
/// 2. Validate using the unresolved (but scope-resolved) state.
/// 3. Fully resolve function bodies against the computed scopes.
pub fn compile_and_resolve(entry_path: &Path) -> Result<Config, CompileError> {
    let abs_entry = canonicalize_entry(entry_path)?;
    let linear_result = resolve_linear(&abs_entry)?;
    validation::validate_configuration(
        &linear_result.unresolved,
        &linear_result.global_scope,
        &linear_result.project_var_scopes,
    )?;
    resolve::resolve_with_scopes(
        linear_result.unresolved,
        linear_result.global_scope,
        linear_result.project_var_scopes,
    )
}

// Linear-processing pipeline

/// Mutable state threaded through the linear processing phase.
struct LinearState {
    global_scope: HashMap<String, String>,
    projects: HashMap<String, UnresolvedProject>,
    /// Per-project locally-declared variable overrides. Each block of the same
    /// project refreshes from current globals (via `or_insert`), so later
    /// global vars are visible across blocks. Project-local vars accumulate
    /// and take priority over globals.
    project_var_scopes: HashMap<String, HashMap<String, String>>,
    loaded_files: HashSet<PathBuf>,
    recursion_stack: HashSet<PathBuf>,
    /// When false, `import` items are silently skipped instead of followed.
    /// Used by `extract_projects` which only needs project fields from the
    /// entry file and must not fail on missing import targets.
    follow_imports: bool,
}

impl LinearState {
    fn new() -> Self {
        Self {
            global_scope: HashMap::new(),
            projects: HashMap::new(),
            project_var_scopes: HashMap::new(),
            loaded_files: HashSet::new(),
            recursion_stack: HashSet::new(),
            follow_imports: true,
        }
    }
}

/// Intermediate result from the linear processing phase.
struct LinearResult {
    unresolved: super::types::UnresolvedConfig,
    global_scope: HashMap<String, String>,
    project_var_scopes: HashMap<String, HashMap<String, String>>,
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
    let base_dir = canon_path
        .parent()
        .map(|parent_path| parent_path.to_path_buf());

    let result = linear_process_program(&program, base_dir.as_deref(), state);

    state.recursion_stack.remove(&canon_path);
    result
}

use crate::dsl::ProjectField;

/// Merge a single statement into a project body during AST collection.
///
/// No expression resolution or shell execution occurs — all values are stored
/// as raw `Expr` nodes and var declarations are stored as raw `Stmt::Var` nodes.
/// Only clones the specific fields needed for storage (key/value for fields,
/// name/body for functions and runs) instead of the entire `Stmt`.
pub(crate) fn merge_project_body_stmt(
    project: &mut UnresolvedProject,
    stmt: &Stmt,
    source_name: &str,
    source_text: &str,
    seen_fields: &mut HashSet<String>,
) -> Result<(), CompileError> {
    let make_err = |msg: String, offset: usize, len: usize| -> CompileError {
        spanned_err(msg, source_name, source_text, offset, len)
    };
    match stmt {
        // Var stmts were already resolved during linear processing and
        // do not need to be stored in the project struct.
        Stmt::Var { .. } => {}
        Stmt::Field {
            key,
            value,
            offset,
            len,
            ..
        } => {
            let field_name = format!("{:?}", key);
            if !seen_fields.insert(field_name) {
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
/// fields, resolve var stmts in the project scope, and collect fn/run blocks.
fn process_project_block(
    name: &str,
    fields: &[Stmt],
    body: &[Stmt],
    project_seen_fields: &mut HashMap<String, HashSet<String>>,
    state: &mut LinearState,
    program: &Program,
) -> Result<(), CompileError> {
    let project_entry =
        state
            .projects
            .entry(name.to_owned())
            .or_insert_with(|| UnresolvedProject {
                name: name.to_owned(),
                url: None,
                dir: None,
                sync: None,
                branch: None,
                functions: HashMap::new(),
                runs: HashMap::new(),
            });

    let seen_fields = project_seen_fields.entry(name.to_owned()).or_default();

    // Refresh project scope from current globals, preserving
    // any project-local vars already declared in prior blocks.
    let project_var_scopes = state.project_var_scopes.entry(name.to_owned()).or_default();
    for (key, value) in &state.global_scope {
        project_var_scopes
            .entry(key.clone())
            .or_insert(value.clone());
    }

    for field_stmt in fields {
        merge_project_body_stmt(
            project_entry,
            field_stmt,
            &program.source_name,
            &program.source_text,
            seen_fields,
        )?;
    }

    for body_stmt in body {
        if let Stmt::Var { .. } = body_stmt {
            resolve::resolve_var_stmt(
                body_stmt,
                project_var_scopes,
                &program.source_name,
                &program.source_text,
            )?;
        }
        merge_project_body_stmt(
            project_entry,
            body_stmt,
            &program.source_name,
            &program.source_text,
            seen_fields,
        )?;
    }
    Ok(())
}

/// Process a program's items in lexical order.
fn linear_process_program(
    program: &Program,
    base_dir: Option<&Path>,
    state: &mut LinearState,
) -> Result<(), CompileError> {
    // Tracks which project fields have been seen per project to detect duplicates.
    let mut project_seen_fields: HashMap<String, HashSet<String>> = HashMap::new();
    for item in &program.items {
        match item {
            TopLevel::Stmt(stmt) => match stmt {
                Stmt::Var { .. } => {
                    resolve::resolve_var_stmt(
                        stmt,
                        &mut state.global_scope,
                        &program.source_name,
                        &program.source_text,
                    )?;
                }
                Stmt::Project {
                    name, fields, body, ..
                } => {
                    process_project_block(
                        name,
                        fields,
                        body,
                        &mut project_seen_fields,
                        state,
                        program,
                    )?;
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
            TopLevel::Import(expr) => {
                if !state.follow_imports {
                    continue;
                }
                let path_str = imports::resolve_import_path(
                    expr,
                    &state.global_scope,
                    &program.source_name,
                    &program.source_text,
                )?;
                let import_path = if Path::new(&path_str).is_absolute() {
                    PathBuf::from(path_str)
                } else if let Some(dir) = base_dir {
                    dir.join(path_str)
                } else {
                    return Err(CompileError::ValidationReport(miette!(
                        "relative import path without base directory"
                    )));
                };
                linear_process_file(&import_path, state)?;
            }
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
        global_scope: state.global_scope,
        project_var_scopes: state.project_var_scopes,
    })
}

/// Lightweight compilation that resolves project metadata without validating
/// or lowering function bodies.
///
/// 1. Linear processing — parse the entry file, resolve `var` and `var shell`
///    declarations, build global and project scopes.  Imports are **skipped**
///    (sync only needs the entry file's project list).
/// 2. Project field resolution — resolve each project's `url`, `dir`, `sync`,
///    and `branch` expressions against its computed scope.
///
/// The returned [`Config`] has empty function maps — function and run
/// blocks are collected during linear processing but never resolved.
pub fn extract_projects(entry_path: &Path) -> Result<Config, CompileError> {
    let abs_entry = canonicalize_entry(entry_path)?;

    let mut state = LinearState {
        follow_imports: false,
        ..LinearState::new()
    };
    linear_process_file(&abs_entry, &mut state)?;

    let mut projects = HashMap::new();
    for (name, unresolved_project) in state.projects {
        let proj_scope = state
            .project_var_scopes
            .get(&name)
            .cloned()
            .unwrap_or_else(|| state.global_scope.clone());

        let sync_offset_len = unresolved_project
            .sync
            .as_ref()
            .map(|e| e.offset_len())
            .unwrap_or((0, 1));

        let url = resolve::resolve_optional_expr(&unresolved_project.url, &proj_scope, "", "")?
            .unwrap_or_default();
        let dir = resolve::resolve_optional_expr(&unresolved_project.dir, &proj_scope, "", "")?
            .unwrap_or_default();
        let sync =
            match resolve::resolve_optional_expr(&unresolved_project.sync, &proj_scope, "", "")? {
                Some(mode) => {
                    let (sync_offset, sync_len) = sync_offset_len;
                    resolve::parse_sync_mode_value(&mode)
                        .map_err(|msg| spanned_err(msg, "", "", sync_offset, sync_len))?
                }
                None => SyncMode::Clone,
            };
        let branch =
            resolve::resolve_optional_expr(&unresolved_project.branch, &proj_scope, "", "")?;

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
