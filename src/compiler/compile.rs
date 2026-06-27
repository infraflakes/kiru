use crate::compiler::error::CompileError;
use crate::compiler::merge;
use crate::compiler::resolve;
use crate::compiler::types::{Sanctuary, UnresolvedProject};
use crate::compiler::validation;
use crate::dsl::Parser;
use crate::dsl::{Expr, FnStmt, Program, Stmt, TopLevel};
use std::collections::HashMap;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// Run the full compilation pipeline:
/// 1. Linear processing: walk items in source order, resolve vars and fields,
///    load imports with variable interpolation, accumulate projects.
/// 2. Validate using the unresolved (but scope-resolved) state.
/// 3. Fully resolve function bodies against the computed scopes.
pub fn compile_and_resolve(entry_path: &Path) -> Result<Sanctuary, CompileError> {
    let abs_entry = canonicalize_entry(entry_path)?;
    let linear_result = resolve_linear(&abs_entry)?;
    validation::validate(
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

// ---------------------------------------------------------------------------
// Linear-processing pipeline
// ---------------------------------------------------------------------------

/// Mutable state threaded through the linear processing phase.
struct LinearState {
    global_scope: HashMap<String, String>,
    sanctuary_path: Option<Expr>,
    projects: HashMap<String, UnresolvedProject>,
    project_seen_fields: HashMap<String, HashSet<String>>,
    config_fns: HashMap<String, Vec<FnStmt>>,
    config_runs: HashMap<String, Vec<Vec<String>>>,
    /// Per-project locally-declared variable overrides. Each block of the same
    /// project refreshes from current globals (via `or_insert`), so later
    /// global vars are visible across blocks. Project-local vars accumulate
    /// and take priority over globals.
    project_var_scopes: HashMap<String, HashMap<String, String>>,
    loaded_files: HashSet<PathBuf>,
    recursion_stack: HashSet<PathBuf>,
}

impl LinearState {
    fn new() -> Self {
        Self {
            global_scope: HashMap::new(),
            sanctuary_path: None,
            projects: HashMap::new(),
            project_seen_fields: HashMap::new(),
            config_fns: HashMap::new(),
            config_runs: HashMap::new(),
            project_var_scopes: HashMap::new(),
            loaded_files: HashSet::new(),
            recursion_stack: HashSet::new(),
        }
    }
}

/// Intermediate result from the linear processing phase.
struct LinearResult {
    unresolved: super::types::UnresolvedSanctuary,
    global_scope: HashMap<String, String>,
    project_var_scopes: HashMap<String, HashMap<String, String>>,
}

/// Canonicalize the entry path, resolving relative paths against the current
/// working directory. All paths within the pipeline are absolute after this.
fn canonicalize_entry(path: &Path) -> Result<PathBuf, CompileError> {
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
            format!("Failed to resolve {}: {}", abs_path.display(), e),
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
        return Err(CompileError::CircularImport(
            canon_path.display().to_string(),
        ));
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

/// Process a program's items in lexical order.
fn linear_process_program(
    program: &Program,
    base_dir: Option<&Path>,
    state: &mut LinearState,
) -> Result<(), CompileError> {
    for item in &program.items {
        match item {
            TopLevel::Stmt(stmt) => {
                match stmt {
                    Stmt::Var { .. } => {
                        resolve::resolve_var_stmt(
                            stmt,
                            &mut state.global_scope,
                            &program.source_name,
                            &program.source_text,
                        )?;
                    }
                    Stmt::Sanctuary { value } => {
                        if state.sanctuary_path.is_none() {
                            state.sanctuary_path = Some(value.clone());
                        }
                    }
                    Stmt::Project { name, body, .. } => {
                        let project_entry =
                            state
                                .projects
                                .entry(name.clone())
                                .or_insert(UnresolvedProject {
                                    name: name.clone(),
                                    url: None,
                                    dir: None,
                                    sync: None,
                                    branch: None,
                                    functions: HashMap::new(),
                                    runs: HashMap::new(),
                                });

                        let seen_fields =
                            state.project_seen_fields.entry(name.clone()).or_default();

                        // Refresh project scope from current globals, preserving
                        // any project-local vars already declared in prior blocks.
                        let project_var_scopes =
                            state.project_var_scopes.entry(name.clone()).or_default();
                        for (key, value) in &state.global_scope {
                            project_var_scopes
                                .entry(key.clone())
                                .or_insert(value.clone());
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
                            merge::merge_project_body_stmt(
                                project_entry,
                                body_stmt.clone(),
                                &program.source_name,
                                &program.source_text,
                                seen_fields,
                            )?;
                        }
                    }
                    Stmt::Fn { name, body, .. } => {
                        if !state.config_fns.contains_key(name) {
                            state.config_fns.insert(name.clone(), body.clone());
                        }
                    }
                    Stmt::Run { name, chains, .. } => {
                        if !state.config_runs.contains_key(name) {
                            state.config_runs.insert(name.clone(), chains.clone());
                        }
                    }
                    Stmt::Field { .. } => {
                        return Err(CompileError::Io(std::io::Error::new(
                            std::io::ErrorKind::InvalidInput,
                            "field outside project block".to_string(),
                        )));
                    }
                }
            }
            TopLevel::Import(expr) => {
                let path_str = resolve_import_path(expr, &state.global_scope)?;
                let import_path = if Path::new(&path_str).is_absolute() {
                    PathBuf::from(path_str)
                } else if let Some(dir) = base_dir {
                    dir.join(path_str)
                } else {
                    return Err(CompileError::Io(std::io::Error::new(
                        std::io::ErrorKind::InvalidInput,
                        "relative import path without base directory".to_string(),
                    )));
                };
                let import_path = canonicalize_entry(&import_path)?;

                linear_process_file(&import_path, state)?;
            }
        }
    }
    Ok(())
}

/// Resolve an import expression with variable interpolation support.
fn resolve_import_path(
    expr: &Expr,
    scope: &HashMap<String, String>,
) -> Result<String, CompileError> {
    match expr {
        Expr::BacktickLit { parts, .. } => {
            let mut path_builder = String::new();
            for part in parts {
                if part.is_var {
                    let resolved_value = scope.get(&part.value).ok_or_else(|| {
                        CompileError::Io(std::io::Error::new(
                            std::io::ErrorKind::InvalidInput,
                            format!("undefined variable in import path: ${{{}}}", part.value),
                        ))
                    })?;
                    path_builder.push_str(resolved_value);
                } else {
                    path_builder.push_str(&part.value);
                }
            }
            if path_builder.is_empty() {
                return Err(CompileError::Io(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    "import path cannot be empty".to_string(),
                )));
            }
            Ok(path_builder)
        }
        Expr::VarRef { name, .. } => {
            let resolved_value = scope.get(name).ok_or_else(|| {
                CompileError::Io(std::io::Error::new(
                    std::io::ErrorKind::InvalidInput,
                    format!("undefined variable in import path: ${}", name),
                ))
            })?;
            Ok(resolved_value.clone())
        }
    }
}

/// The core linear processing phase: entry point.
fn resolve_linear(entry_path: &Path) -> Result<LinearResult, CompileError> {
    let mut state = LinearState::new();
    linear_process_file(entry_path, &mut state)?;

    let unresolved = super::types::UnresolvedSanctuary {
        sanctuary_path: state.sanctuary_path,
        projects: state.projects,
        functions: state.config_fns,
        runs: state.config_runs,
    };

    Ok(LinearResult {
        unresolved,
        global_scope: state.global_scope,
        project_var_scopes: state.project_var_scopes,
    })
}
