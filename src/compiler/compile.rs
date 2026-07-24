use crate::compiler::error::{CompileError, io_err, spanned_err_named, spanned_err_on_field};
use crate::compiler::fnstmt::{resolve_fn_body_stmts, validate_fn_body_stmts};

use crate::compiler::namespaces::{Namespaces, resolve_expr};
use crate::dsl::Parser;
use crate::dsl::lexer::Lexer;
use crate::dsl::{Expr, Program, ProjectField, Stmt, TopLevel, VarType, ast::QualifiedFnRef};
use crate::error::SourceFile;
use crate::error::spanned_report;
use crate::plan::{Plan, PlanProject, PlanStmt, parse_sync_mode};
use crate::shell::execute_shell_variable;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};

/// Run the full compilation pipeline in one eager pass:
/// 1. Walk items in source order, resolving globals as they are encountered.
/// 2. For each `pr` block: resolve fields immediately (globals only), then
///    process body statements eagerly — `var`/`var shell` are resolved at
///    their declaration point, `use fn` clones the global template, rewrites
///    `self::`, validates, and resolves the body immediately (case arms are
///    compile-time matched). At the end, `build_plan` returns a `Plan`
///    with every project's functions already lowered to `PlanStmt`.
pub fn compile_and_resolve(entry_path: &Path, force_cwd: bool) -> Result<Plan, CompileError> {
    let abs_entry = canonicalize_entry(entry_path)?;
    let mut state = LinearState::new(false, force_cwd, true);
    linear_process_file(&abs_entry, &mut state)?;
    build_plan(state)
}

/// Lightweight compilation that resolves project metadata without lowering
/// function bodies. Used by `kiru sync`, which only needs each project's
/// `url`/`dir`/`sync`/`branch`. Imports are tolerant of not-yet-cloned
/// repositories (`skip_missing = true`).
pub fn parse_projects_metadata(entry_path: &Path) -> Result<Plan, CompileError> {
    let abs_entry = canonicalize_entry(entry_path)?;
    let mut state = LinearState::new(true, false, false);
    linear_process_file(&abs_entry, &mut state)?;
    build_plan(state)
}

// ── Mutable state ──────────────────────────────────────────────────────────

struct LinearState {
    namespaces: Namespaces,
    projects: BTreeMap<String, PendingProject>,
    global_functions: BTreeMap<String, Vec<crate::dsl::FnStmt>>,
    runs: BTreeMap<String, Vec<Vec<QualifiedFnRef>>>,
    loaded_files: HashSet<PathBuf>,
    recursion_stack: HashSet<PathBuf>,
    skip_missing: bool,
    force_cwd: bool,
    lower_functions: bool,
    source_texts: HashMap<String, String>,
}

/// A project being accumulated across merged `pr` blocks.
/// Fields are resolved `String`s; `None` means not yet set (duplicate
/// detection for merges). Functions are resolved `PlanStmt` arrays when
/// `lower_functions` is true, otherwise empty.
struct PendingProject {
    source_file: String,
    url: Option<String>,
    dir: Option<String>,
    sync: Option<String>,
    branch: Option<String>,
    functions: BTreeMap<String, Vec<PlanStmt>>,
}

impl LinearState {
    fn new(skip_missing: bool, force_cwd: bool, lower_functions: bool) -> Self {
        Self {
            namespaces: Namespaces::new(),
            projects: BTreeMap::new(),
            global_functions: BTreeMap::new(),
            loaded_files: HashSet::new(),
            recursion_stack: HashSet::new(),
            skip_missing,
            force_cwd,
            lower_functions,
            source_texts: HashMap::new(),
            runs: BTreeMap::new(),
        }
    }
}

// ── Plan assembly ──────────────────────────────────────────────────────────

fn build_plan(state: LinearState) -> Result<Plan, CompileError> {
    let mut projects = BTreeMap::new();
    for (name, p) in state.projects {
        let sync_str = p.sync.unwrap_or_else(|| "clone".to_string());
        let sync = parse_sync_mode(&sync_str)
            .map_err(|msg| spanned_err_on_field(msg, &state.source_texts, &None, &p.source_file))?;
        projects.insert(
            name,
            PlanProject {
                url: p.url.unwrap_or_default(),
                dir: p.dir.unwrap_or_default(),
                sync,
                branch: p.branch,
                functions: p.functions,
            },
        );
    }
    Ok(Plan {
        projects,
        runs: state.runs,
    })
}

// ── File / import processing ───────────────────────────────────────────────

pub(crate) fn canonicalize_entry(path: &Path) -> Result<PathBuf, CompileError> {
    let abs_path = if path.is_absolute() {
        path.to_path_buf()
    } else {
        std::env::current_dir()
            .map_err(CompileError::Io)?
            .join(path)
    };
    std::fs::canonicalize(&abs_path).map_err(|e| io_err("Failed to resolve", &abs_path, &e))
}

fn parse_file(
    canon_path: &Path,
    loaded_files: &mut HashSet<PathBuf>,
) -> Result<Program, CompileError> {
    if loaded_files.contains(canon_path) {
        return Ok(Program::new());
    }
    let data = std::fs::read_to_string(canon_path)
        .map_err(|e| io_err("Failed to read", canon_path, &e))?;
    let source_name = canon_path.display().to_string();
    let source_text = data.clone();
    let mut parser = Parser::new(Lexer::new(data)).with_source_name(source_name.clone());
    let mut program = Program::new_with_source(source_name, source_text);
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

fn linear_process_file(file_path: &Path, state: &mut LinearState) -> Result<(), CompileError> {
    let canon_path =
        std::fs::canonicalize(file_path).map_err(|e| io_err("Failed to resolve", file_path, &e))?;
    if state.recursion_stack.contains(&canon_path) {
        return Err(spanned_err_named(
            format!("circular import: {}", canon_path.display()),
            &state.source_texts,
            &canon_path.display().to_string(),
            0,
            1,
        ));
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

fn process_import(
    expr: &Expr,
    state: &mut LinearState,
    program: &Program,
) -> Result<(), CompileError> {
    let mut expr = expr.clone();
    let mut scope_errors = Vec::new();
    crate::compiler::scope::normalize_expr(
        &mut expr,
        crate::compiler::scope::GLOBAL_SCOPE,
        &state.source_texts,
        &mut scope_errors,
    );
    if !scope_errors.is_empty() {
        return Err(CompileError::ValidationReport(scope_errors));
    }
    let path_str = resolve_expr(&expr, &state.namespaces, &state.source_texts)?;
    if path_str.is_empty() {
        let (offset, len) = expr.offset_len();
        return Err(spanned_err_named(
            "import path cannot be empty".to_string(),
            &state.source_texts,
            &program.source_name,
            offset,
            len,
        ));
    }
    let (import_offset, import_len) = expr.offset_len();
    let import_source = expr.source_name().to_string();
    let base_dir = Path::new(&program.source_name).parent().ok_or_else(|| {
        spanned_err_named(
            format!(
                "cannot determine base directory for import from '{}'",
                program.source_name
            ),
            &state.source_texts,
            &import_source,
            import_offset,
            import_len,
        )
    })?;
    let target = base_dir.join(&path_str);
    if state.skip_missing && !target.exists() {
        let report = spanned_report(
            format!(
                "import target '{}' does not exist yet (from {}), skipping",
                path_str, program.source_name
            ),
            &SourceFile::from_registry(&state.source_texts, &import_source),
            import_offset,
            import_len,
        );
        eprintln!("{:?}", report);
        return Ok(());
    }
    linear_process_file(&target, state)
}

// ── Project block ──────────────────────────────────────────────────────────

fn process_project_block(
    name: &str,
    fields: &[Stmt],
    body: &[Stmt],
    state: &mut LinearState,
    program: &Program,
) -> Result<(), CompileError> {
    // Register the project namespace immediately.
    state.namespaces.declare_project(name)?;

    // Get or create the pending project entry.
    let project = state
        .projects
        .entry(name.to_owned())
        .or_insert_with(|| PendingProject {
            source_file: program.source_name.clone(),
            url: None,
            dir: None,
            sync: None,
            branch: None,
            functions: BTreeMap::new(),
        });

    // ── Process fields ──────────────────────────────────────────────────
    for field_stmt in fields {
        if let Stmt::Field {
            key,
            value,
            offset,
            len,
            ..
        } = field_stmt
        {
            // Normalize (rewrite self:: → project name) and resolve eagerly.
            let mut value = value.clone();
            let mut scope_errors = Vec::new();
            crate::compiler::scope::normalize_expr(
                &mut value,
                name,
                &state.source_texts,
                &mut scope_errors,
            );
            if !scope_errors.is_empty() {
                return Err(CompileError::ValidationReport(scope_errors));
            }
            let resolved = resolve_expr(&value, &state.namespaces, &state.source_texts)?;
            match key {
                ProjectField::Dir => {
                    if project.dir.is_some() {
                        return Err(spanned_err_named(
                            format!("duplicate field 'dir' in project '{}'", name),
                            &state.source_texts,
                            &program.source_name,
                            *offset,
                            *len,
                        ));
                    }
                    // Resolve relative paths against the source file.
                    let dir = if resolved.is_empty() || Path::new(&resolved).is_absolute() {
                        resolved
                    } else {
                        let base = Path::new(&program.source_name).parent().ok_or_else(|| {
                            spanned_err_named(
                                "cannot determine base directory for dir".to_string(),
                                &state.source_texts,
                                &program.source_name,
                                *offset,
                                *len,
                            )
                        })?;
                        base.join(&resolved).to_string_lossy().to_string()
                    };
                    project.dir = Some(dir);
                }
                ProjectField::Url => {
                    if project.url.is_some() {
                        return Err(spanned_err_named(
                            format!("duplicate field 'url' in project '{}'", name),
                            &state.source_texts,
                            &program.source_name,
                            *offset,
                            *len,
                        ));
                    }
                    project.url = Some(resolved);
                }
                ProjectField::Sync => {
                    if project.sync.is_some() {
                        return Err(spanned_err_named(
                            format!("duplicate field 'sync' in project '{}'", name),
                            &state.source_texts,
                            &program.source_name,
                            *offset,
                            *len,
                        ));
                    }
                    project.sync = Some(resolved);
                }
                ProjectField::Branch => {
                    if project.branch.is_some() {
                        return Err(spanned_err_named(
                            format!("duplicate field 'branch' in project '{}'", name),
                            &state.source_texts,
                            &program.source_name,
                            *offset,
                            *len,
                        ));
                    }
                    project.branch = Some(resolved);
                }
            }
        }
    }

    // Compute working_dir for var shell execution and function body resolution.
    let dir = project.dir.clone().unwrap_or_default();
    let working_dir: Option<PathBuf> = if state.force_cwd || dir.is_empty() {
        None
    } else {
        Some(PathBuf::from(&dir))
    };
    let working_dir_ref = working_dir.as_deref();

    // ── Process body statements eagerly ─────────────────────────────────
    for body_stmt in body {
        if let Stmt::Use {
            function,
            alias,
            offset,
            len,
            source_name,
            ..
        } = body_stmt
        {
            let bound_name = alias.clone().unwrap_or_else(|| function.clone());
            let global_body = state.global_functions.get(function).ok_or_else(|| {
                spanned_err_named(
                    format!("unknown global function: `{}`", function),
                    &state.source_texts,
                    source_name,
                    *offset,
                    *len,
                )
            })?;

            if project.functions.contains_key(&bound_name) {
                return Err(spanned_err_named(
                    format!(
                        "duplicate function in project '{}': {} (also applied via `use`)",
                        name, bound_name
                    ),
                    &state.source_texts,
                    source_name,
                    *offset,
                    *len,
                ));
            }

            // Eager check: every `self::name` the function reads must be
            // supplied by the applying project, unless it is the function's
            // own local variable.
            let mut self_vars = std::collections::HashSet::new();
            for stmt in global_body {
                stmt.visit_vars(&mut |name, namespace| {
                    if namespace == "self" {
                        self_vars.insert(name.to_string());
                    }
                });
            }
            let mut local_vars = std::collections::HashSet::new();
            collect_fn_local_var_names(global_body, &mut local_vars);
            let missing: Vec<&String> = self_vars
                .difference(&local_vars)
                .filter(|var_name| !state.namespaces.project_var_exists(name, var_name))
                .collect();
            if !missing.is_empty() {
                return Err(spanned_err_named(
                    format!(
                        "function `{}` requires variable(s) {{{}}} that project `{}` does not declare before this `use` (kiru is strictly top-down)",
                        function,
                        missing
                            .iter()
                            .map(|n| n.as_str())
                            .collect::<Vec<_>>()
                            .join(", "),
                        name
                    ),
                    &state.source_texts,
                    source_name,
                    *offset,
                    *len,
                ));
            }

            // Clone and rewrite `self::` to the destination project.
            // Also remap every expression's source span to point to the `use`
            // statement so that errors from resolving the body reference the
            // applying project, not the global template.
            let mut body = global_body.clone();
            for stmt in &mut body {
                stmt.remap_source_span(source_name, *offset, *len);
            }
            let mut scope_errors = Vec::new();
            for stmt in &mut body {
                stmt.visit_namespaces_mut(&mut |namespace, offset, len, src| {
                    crate::compiler::scope::rewrite_and_check(
                        namespace,
                        name,
                        offset,
                        len,
                        src,
                        &state.source_texts,
                        &mut scope_errors,
                    );
                });
            }
            if !scope_errors.is_empty() {
                return Err(CompileError::ValidationReport(scope_errors));
            }

            // Declare function-local vars, checking collisions with project vars.
            let mut fn_local_names: Vec<String> = Vec::new();
            declare_fn_body_vars_inner(
                &mut state.namespaces,
                name,
                &body,
                &mut fn_local_names,
                &state.source_texts,
                *offset,
                *len,
                source_name,
            )?;

            // Validate and then resolve the function body.
            let mut errors = Vec::new();
            let mut validation_ctx = crate::compiler::fnstmt::ValidateFnCtx {
                fn_name: &bound_name,
                proj_name: name,
                namespaces: &state.namespaces,
                errors: &mut errors,
                sources: &state.source_texts,
            };
            validate_fn_body_stmts(&body, &mut validation_ctx);
            if !errors.is_empty() {
                return Err(CompileError::ValidationReport(errors));
            }

            if state.lower_functions {
                let resolved_body = resolve_fn_body_stmts(
                    &body,
                    &mut state.namespaces,
                    name,
                    working_dir_ref,
                    &state.source_texts,
                )?;
                project.functions.insert(bound_name, resolved_body);
            } else {
                // Register the function name with an empty body so run-block
                // validation can find it even when function lowering is off
                // (e.g. during `kiru sync`).
                project.functions.insert(bound_name, Vec::new());
            }
            continue;
        }

        if let Stmt::Var {
            var_type,
            name: var_name,
            value,
            offset,
            len,
            ..
        } = body_stmt
        {
            // Normalize (rewrite self:: → project name) and resolve eagerly.
            let mut value = value.clone();
            let mut scope_errors = Vec::new();
            crate::compiler::scope::normalize_expr(
                &mut value,
                name,
                &state.source_texts,
                &mut scope_errors,
            );
            if !scope_errors.is_empty() {
                return Err(CompileError::ValidationReport(scope_errors));
            }
            let resolved = resolve_expr(&value, &state.namespaces, &state.source_texts)?;
            let final_value = if *var_type == VarType::Shell {
                let source = SourceFile::from_registry(&state.source_texts, value.source_name());
                execute_shell_variable(
                    var_name,
                    &resolved,
                    working_dir_ref,
                    &source,
                    *offset,
                    *len,
                )?
            } else {
                resolved
            };
            state.namespaces.declare_project_var(
                name,
                var_name,
                final_value,
                &program.source_name,
                *offset,
                *len,
                &state.source_texts,
            )?;
        }
    }

    Ok(())
}

// ── Function body helpers ──────────────────────────────────────────────────

fn collect_fn_local_var_names(
    stmts: &[crate::dsl::FnStmt],
    out: &mut std::collections::HashSet<String>,
) {
    for stmt in stmts {
        match stmt {
            crate::dsl::FnStmt::VarDecl(s) => {
                out.insert(s.name.clone());
            }
            crate::dsl::FnStmt::EnvBlock(s) => collect_fn_local_var_names(&s.body, out),
            crate::dsl::FnStmt::Case(s) => {
                for arm in &s.scopes {
                    collect_fn_local_var_names(&arm.body, out);
                }
            }
            _ => {}
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn declare_fn_body_vars_inner(
    namespaces: &mut Namespaces,
    project_name: &str,
    stmts: &[crate::dsl::FnStmt],
    current_locals: &mut Vec<String>,
    source_texts: &HashMap<String, String>,
    use_offset: usize,
    use_len: usize,
    use_source: &str,
) -> Result<(), CompileError> {
    for stmt in stmts {
        match stmt {
            crate::dsl::FnStmt::VarDecl(s) => {
                if namespaces.project_var_exists(project_name, &s.name)
                    || current_locals.iter().any(|n| n == &s.name)
                {
                    return Err(spanned_err_named(
                        format!(
                            "function local variable `{}` collides with a variable already declared in project `{}` or this function (rename the function's local variable)",
                            s.name, project_name
                        ),
                        source_texts,
                        use_source,
                        use_offset,
                        use_len,
                    ));
                }
                current_locals.push(s.name.clone());
                // Also declare an empty placeholder so validation of
                // sibling arms sees this name as defined. The real value
                // will be set by resolve_fn_body_stmts when the matching
                // case arm is resolved.
                let _ = namespaces.declare_project_var(
                    project_name,
                    &s.name,
                    String::new(),
                    use_source,
                    use_offset,
                    use_len,
                    source_texts,
                );
            }
            crate::dsl::FnStmt::EnvBlock(s) => {
                declare_fn_body_vars_inner(
                    namespaces,
                    project_name,
                    &s.body,
                    current_locals,
                    source_texts,
                    use_offset,
                    use_len,
                    use_source,
                )?;
            }
            crate::dsl::FnStmt::Case(s) => {
                for arm in &s.scopes {
                    declare_fn_body_vars_inner(
                        namespaces,
                        project_name,
                        &arm.body,
                        current_locals,
                        source_texts,
                        use_offset,
                        use_len,
                        use_source,
                    )?;
                }
            }
            _ => {}
        }
    }
    Ok(())
}

// ── Top-level program processing ───────────────────────────────────────────

fn linear_process_program(program: &Program, state: &mut LinearState) -> Result<(), CompileError> {
    state
        .source_texts
        .insert(program.source_name.clone(), program.source_text.clone());
    for item in &program.items {
        match item {
            TopLevel::Stmt(stmt) => match stmt {
                Stmt::Var {
                    var_type,
                    name,
                    value,
                    offset,
                    len,
                    ..
                } => {
                    let mut value = value.clone();
                    let mut scope_errors = Vec::new();
                    crate::compiler::scope::normalize_expr(
                        &mut value,
                        crate::compiler::scope::GLOBAL_SCOPE,
                        &state.source_texts,
                        &mut scope_errors,
                    );
                    if !scope_errors.is_empty() {
                        return Err(CompileError::ValidationReport(scope_errors));
                    }
                    let resolved = resolve_expr(&value, &state.namespaces, &state.source_texts)?;
                    let final_value = if *var_type == VarType::Shell {
                        let source =
                            SourceFile::from_registry(&state.source_texts, value.source_name());
                        execute_shell_variable(name, &resolved, None, &source, *offset, *len)?
                    } else {
                        resolved
                    };
                    state.namespaces.declare_global(
                        name,
                        final_value,
                        &program.source_name,
                        *offset,
                        *len,
                        &state.source_texts,
                    )?;
                }
                Stmt::Project {
                    name, fields, body, ..
                } => {
                    process_project_block(name, fields, body, state, program)?;
                }
                Stmt::Fn {
                    name,
                    body,
                    offset,
                    len,
                    ..
                } => {
                    if state.global_functions.contains_key(name) {
                        return Err(spanned_err_named(
                            format!("duplicate global function: {}", name),
                            &state.source_texts,
                            &program.source_name,
                            *offset,
                            *len,
                        ));
                    }
                    let mut body = body.clone();
                    let mut scope_errors = Vec::new();
                    for stmt in &mut body {
                        stmt.visit_namespaces_mut(&mut |namespace, offset, len, src| {
                            crate::compiler::scope::rewrite_and_check(
                                namespace,
                                crate::compiler::scope::TEMPLATE_SCOPE,
                                offset,
                                len,
                                src,
                                &state.source_texts,
                                &mut scope_errors,
                            );
                        });
                    }
                    if !scope_errors.is_empty() {
                        return Err(CompileError::ValidationReport(scope_errors));
                    }
                    state.global_functions.insert(name.clone(), body);
                }
                Stmt::Use { offset, len, .. } => {
                    return Err(spanned_err_named(
                        "function applications (`use`) are only valid inside a project body"
                            .to_string(),
                        &state.source_texts,
                        &program.source_name,
                        *offset,
                        *len,
                    ));
                }
                Stmt::Run {
                    name,
                    chains,
                    offset,
                    len,
                    ..
                } => {
                    if state.runs.contains_key(name) {
                        return Err(spanned_err_named(
                            format!("duplicate run block: {}", name),
                            &state.source_texts,
                            &program.source_name,
                            *offset,
                            *len,
                        ));
                    }
                    let mut chains = chains.clone();
                    for chain in &mut chains {
                        for reference in chain {
                            if reference.project == "self" {
                                reference.project = "global".to_string();
                            }
                            match state.projects.get(&reference.project) {
                                Some(proj) => {
                                    if !proj.functions.contains_key(&reference.function) {
                                        return Err(spanned_err_named(
                                            format!(
                                                "run {:?}: function {:?} not found in project {:?}",
                                                name, reference.function, reference.project
                                            ),
                                            &state.source_texts,
                                            &reference.source_name,
                                            reference.offset,
                                            reference.len,
                                        ));
                                    }
                                }
                                None => {
                                    return Err(spanned_err_named(
                                        format!(
                                            "run {:?}: unknown project {:?}",
                                            name, reference.project
                                        ),
                                        &state.source_texts,
                                        &reference.source_name,
                                        reference.offset,
                                        reference.len,
                                    ));
                                }
                            }
                        }
                    }
                    state.runs.insert(name.clone(), chains);
                }
                _ => {}
            },
            TopLevel::Import(expr) => process_import(expr, state, program)?,
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::compiler::test_support::*;
    use crate::compiler::{CompileError, parse_projects_metadata};

    #[test]
    fn test_cross_file_redeclaration_points_at_defining_file() {
        let dir = tempfile::TempDir::new().unwrap();
        write_config(
            dir.path(),
            "main.kiru",
            "\
            import `.kiru/run.kiru`;\n\
            import `.kiru/build.kiru`;\n\
            pr kiru [url = `u` dir = `d`] { }\n\
            ",
        );
        std::fs::create_dir_all(dir.path().join(".kiru")).unwrap();
        write_config(
            &dir.path().join(".kiru"),
            "run.kiru",
            "fn all { log `hi`; }\npr kiru { use all; }\n",
        );
        write_config(
            &dir.path().join(".kiru"),
            "build.kiru",
            "\
            fn build_with_container {\n\
                var string docker_bin = `docker`;\n\
            }\n\
            pr kiru {\n\
                var string docker_bin = `docker`;\n\
                use build_with_container;\n\
            }\n\
            ",
        );
        let err = compile_full(&dir.path().join("main.kiru")).unwrap_err();
        let rendered = match &err {
            CompileError::ValidationReport(reports) => format!("{:?}", reports[0]),
            other => other.to_string(),
        };
        assert!(
            rendered.contains("docker_bin"),
            "expected redeclaration diagnostic, got: {}",
            rendered
        );
        assert!(
            rendered.contains("build.kiru"),
            "diagnostic should point at build.kiru, got: {}",
            rendered
        );
        assert!(
            !rendered.contains("run.kiru"),
            "diagnostic must not point at run.kiru, got: {}",
            rendered
        );
    }

    #[test]
    fn test_import_path_depends_on_global_var_shell() {
        let dir = tempfile::TempDir::new().unwrap();
        let base = dir.path().canonicalize().unwrap();
        std::fs::create_dir_all(base.join("sub")).unwrap();
        write_config(
            &base.join("sub"),
            "imported.kiru",
            "pr fromimport [url = `u` dir = `d`] { }\n",
        );
        write_config(
            &base,
            "main.kiru",
            &format!(
                "\
             var shell root = `echo {}`;\n\
             var string subdir = `${{global::root}}/sub`;\n\
             import `${{global::subdir}}/imported.kiru`;\n\
             ",
                base.to_string_lossy()
            ),
        );
        let cfg = compile_full(&base.join("main.kiru")).unwrap();
        assert!(
            cfg.projects.contains_key("fromimport"),
            "import whose path depends on a global var shell should load"
        );
    }

    #[test]
    fn test_load_basic() {
        let dir = tempfile::TempDir::new().unwrap();
        write_config(
            dir.path(),
            "main.kiru",
            "\
         var string a = `hello`;\n\
         pr test [url = `http://example.com` dir = `test`] { }\
         ",
        );
        let cfg = compile_full(&dir.path().join("main.kiru")).unwrap();
        assert!(cfg.projects.contains_key("test"));
        assert_eq!(cfg.projects["test"].url, "http://example.com");
    }

    #[test]
    fn test_load_with_project_body() {
        let dir = tempfile::TempDir::new().unwrap();
        write_config(
            dir.path(),
            "main.kiru",
            "\
         fn build { log `hi`; }\n\
         pr test [\n\
             url = `http://example.com`\n\
             dir = `test`\n\
         ] {\n\
             var string app = `todo`;\n\
             use build;\n\
         }\n\
         run release { test::build; }\n\
         run ci { test::build; }\n\
         ",
        );
        let cfg = compile_full(&dir.path().join("main.kiru")).unwrap();
        let proj = &cfg.projects["test"];
        assert!(proj.functions.contains_key("build"));
        assert!(cfg.runs.contains_key("release"));
        assert!(cfg.runs.contains_key("ci"));
        assert_eq!(cfg.runs.len(), 2);
        let check_run = |name: &str, expected_project: &str, expected_fn: &str| {
            let chains = &cfg.runs[name];
            assert_eq!(chains.len(), 1, "run {name} chain count");
            assert_eq!(chains[0].len(), 1, "run {name} ref count");
            assert_eq!(chains[0][0].project, expected_project, "run {name} project");
            assert_eq!(chains[0][0].function, expected_fn, "run {name} function");
        };
        check_run("release", "test", "build");
        check_run("ci", "test", "build");
    }

    #[test]
    fn test_import_resolution() {
        let dir = tempfile::TempDir::new().unwrap();
        write_config(dir.path(), "other.kiru", "var string extra = `from-other`;");
        write_config(
            dir.path(),
            "main.kiru",
            "\
         import `./other.kiru`;\n\
         pr p [url = $global::extra dir = `d`] { }
         ",
        );
        let cfg = compile_full(&dir.path().join("main.kiru")).unwrap();
        assert_eq!(cfg.projects["p"].url, "from-other");
    }

    #[test]
    fn test_circular_import() {
        let dir = tempfile::TempDir::new().unwrap();
        write_config(dir.path(), "a.kiru", "import `./b.kiru`;");
        write_config(dir.path(), "b.kiru", "import `./a.kiru`;");
        let err = compile_full(&dir.path().join("a.kiru")).unwrap_err();
        let err_str = err.to_string();
        assert!(
            err_str.contains("circular") || err_str.contains("Circular"),
            "got: {}",
            err_str
        );
    }

    #[test]
    fn test_duplicate_project_merges() {
        let dir = tempfile::TempDir::new().unwrap();
        write_config(
            dir.path(),
            "main.kiru",
            "\
         fn build { log `x`; }\n\
         pr p1 [url = `u` dir = `d1`] { }\n\
         pr p1 { use build; }\
         ",
        );
        let cfg = compile_full(&dir.path().join("main.kiru")).unwrap();
        assert!(cfg.projects.contains_key("p1"));
        let proj = &cfg.projects["p1"];
        assert_eq!(proj.url, "u");
        assert_eq!(proj.dir, dir.path().join("d1").to_string_lossy());
        assert!(proj.functions.contains_key("build"));
    }

    #[test]
    fn test_missing_url() {
        let dir = tempfile::TempDir::new().unwrap();
        write_config(
            dir.path(),
            "main.kiru",
            "\
         pr p [dir = `d`] { }\
         ",
        );
        compile_full(&dir.path().join("main.kiru")).unwrap();
    }

    #[test]
    fn test_missing_dir() {
        let dir = tempfile::TempDir::new().unwrap();
        write_config(
            dir.path(),
            "main.kiru",
            "\
         pr p [url = `u`] { }\
         ",
        );
        compile_full(&dir.path().join("main.kiru")).unwrap();
    }

    #[test]
    fn test_multi_file_parse_order() {
        let dir = tempfile::TempDir::new().unwrap();
        write_config(dir.path(), "a.kiru", "var string a = `from-a`;");
        write_config(
            dir.path(),
            "main.kiru",
            "\
         import `./a.kiru`;\n\
         pr p [url = $global::a dir = `d`] { }\
         ",
        );
        let cfg = compile_full(&dir.path().join("main.kiru")).unwrap();
        assert_eq!(cfg.projects["p"].url, "from-a");
    }

    #[test]
    fn test_duplicate_project_field() {
        let dir = tempfile::TempDir::new().unwrap();
        write_config(
            dir.path(),
            "main.kiru",
            "\
         pr p [url = `u` dir = `d` dir = `e`] { }\
         ",
        );
        let err = compile_full(&dir.path().join("main.kiru")).unwrap_err();
        assert!(err.to_string().contains("duplicate"), "got: {}", err);
    }

    #[test]
    fn test_project_fn_collection() {
        let dir = tempfile::TempDir::new().unwrap();
        write_config(
            dir.path(),
            "main.kiru",
            "\
         fn build { log `building`; }\n\
         fn test { exec `check`; }\n\
         pr p [ url = `http://x` dir = `x` ] {\n\
             use build;\n\
             use test;\n\
         }\
         ",
        );
        let cfg = compile_full(&dir.path().join("main.kiru")).unwrap();
        let proj = &cfg.projects["p"];
        assert!(proj.functions.contains_key("build"));
        assert!(proj.functions.contains_key("test"));
        assert_eq!(proj.functions.len(), 2);
    }

    #[test]
    fn test_project_run_collection() {
        let dir = tempfile::TempDir::new().unwrap();
        write_config(
            dir.path(),
            "main.kiru",
            "\
         fn build { log `x`; }\n\
         fn test { log `y`; }\n\
         pr p [ url = `http://x` dir = `x` ] {\n\
             use build;\n\
             use test;\n\
         }\n\
         run all { p::build => p::test; }\n\
         run ci { p::build; }\n\
         ",
        );
        let cfg = compile_full(&dir.path().join("main.kiru")).unwrap();
        assert!(cfg.runs.contains_key("all"));
        assert!(cfg.runs.contains_key("ci"));
        assert_eq!(cfg.runs.len(), 2);
        let check_chain = |name: &str, expected: &[(&str, &str)]| {
            let chains = &cfg.runs[name];
            assert_eq!(chains.len(), 1, "run {name} chain count");
            let actual: Vec<(&str, &str)> = chains[0]
                .iter()
                .map(|q| (q.project.as_str(), q.function.as_str()))
                .collect();
            assert_eq!(actual, expected, "run {name}");
        };
        check_chain("all", &[("p", "build"), ("p", "test")]);
    }

    #[test]
    fn test_duplicate_fn_in_project() {
        let dir = tempfile::TempDir::new().unwrap();
        write_config(
            dir.path(),
            "main.kiru",
            "\
         fn dup { log `a`; }\n\
         pr test [\n\
             url = `u`\n\
             dir = `d`\n\
         ] {\n\
             use dup;\n\
             use dup;\n\
         }\
         ",
        );
        let err = compile_full(&dir.path().join("main.kiru")).unwrap_err();
        assert!(
            err.to_string().contains("duplicate function"),
            "got: {}",
            err
        );
    }

    #[test]
    fn test_duplicate_run_in_project() {
        let dir = tempfile::TempDir::new().unwrap();
        write_config(
            dir.path(),
            "main.kiru",
            "\
         fn check { log `x`; }\n\
         pr test [\n\
             url = `u`\n\
             dir = `d`\n\
         ] {\n\
             use check;\n\
         }\n\
         run dup { test::check; }\n\
         run dup { test::check; }\n\
         ",
        );
        let err = compile_full(&dir.path().join("main.kiru")).unwrap_err();
        assert!(
            err.to_string().contains("duplicate run block"),
            "got: {}",
            err
        );
    }

    #[test]
    fn test_duplicate_fn_in_project_errors() {
        let dir = tempfile::TempDir::new().unwrap();
        write_config(
            dir.path(),
            "main.kiru",
            "\
         fn dup { log `a`; }\n\
         pr p [ url = `http://x` dir = `x` ] {\n\
             use dup;\n\
             use dup;\n\
         }\
         ",
        );
        let err = compile_full(&dir.path().join("main.kiru")).unwrap_err();
        assert!(
            err.to_string().contains("duplicate function"),
            "got: {}",
            err
        );
    }

    #[test]
    fn test_duplicate_run_in_project_errors() {
        let dir = tempfile::TempDir::new().unwrap();
        write_config(
            dir.path(),
            "main.kiru",
            "\
         fn x { log `a`; }\n\
         pr p [ url = `http://x` dir = `x` ] {\n\
             use x;\n\
         }\n\
         run dup { p::x; }\n\
         run dup { p::x; }\n\
         ",
        );
        let err = compile_full(&dir.path().join("main.kiru")).unwrap_err();
        assert!(
            err.to_string().contains("duplicate run block"),
            "got: {}",
            err
        );
    }

    #[test]
    fn test_parse_metadata_skips_missing_import() {
        let dir = tempfile::TempDir::new().unwrap();
        write_config(
            dir.path(),
            "main.kiru",
            "\
         import `./missing.kiru`;\n\
         pr myproj [url = `http://example.com` dir = `d`] { }\
         ",
        );
        let cfg = parse_projects_metadata(&dir.path().join("main.kiru")).unwrap();
        let proj = &cfg.projects["myproj"];
        assert_eq!(proj.url, "http://example.com");
    }

    #[test]
    fn test_compile_and_resolve_strict_missing_import_errors() {
        let dir = tempfile::TempDir::new().unwrap();
        write_config(
            dir.path(),
            "main.kiru",
            "\
         import `./missing.kiru`;\n\
         pr myproj [url = `u` dir = `d`] { }\
         ",
        );
        let err = compile_full(&dir.path().join("main.kiru")).unwrap_err();
        let err_str = err.to_string();
        assert!(err_str.contains("Failed to resolve"), "got: {}", err_str);
    }

    #[test]
    fn test_parse_metadata_still_errors_on_malformed_import() {
        let dir = tempfile::TempDir::new().unwrap();
        write_config(
            dir.path(),
            "bad.kiru",
            "\
         var string x = ;\
         ",
        );
        write_config(
            dir.path(),
            "main.kiru",
            "\
         import `./bad.kiru`;\n\
         pr p [url = `u` dir = `d`] { }\
         ",
        );
        let err = parse_projects_metadata(&dir.path().join("main.kiru")).unwrap_err();
        assert!(
            matches!(err, CompileError::ParseReports(_)),
            "expected a parse error, got: {}",
            err
        );
    }

    #[test]
    fn test_use_function_instantiates_into_project() {
        let dir = tempfile::TempDir::new().unwrap();
        write_config(
            dir.path(),
            "main.kiru",
            "\
         fn shared { log `shared body`; }\n\
         pr a [url = `u` dir = `da`] { use shared; }\n\
         pr b [url = `u` dir = `db`] { use shared; }\n\
         ",
        );
        let cfg = compile_full(&dir.path().join("main.kiru")).unwrap();
        assert!(cfg.projects["a"].functions.contains_key("shared"));
        assert!(cfg.projects["b"].functions.contains_key("shared"));
        assert!(cfg.runs.is_empty());
    }

    #[test]
    fn test_use_function_runs_with_project_cwd() {
        let dir = tempfile::TempDir::new().unwrap();
        write_config(
            dir.path(),
            "main.kiru",
            "\
         var string who = `world`;\n\
         fn greet { log `hi from ${global::who}`; }\n\
         pr p [url = `u` dir = `d`] {\n\
             use greet;\n\
         }\n\
         ",
        );
        let cfg = compile_full(&dir.path().join("main.kiru")).unwrap();
        assert!(cfg.projects["p"].functions.contains_key("greet"));
    }

    #[test]
    fn test_use_function_duplicate_is_error() {
        let dir = tempfile::TempDir::new().unwrap();
        write_config(
            dir.path(),
            "main.kiru",
            "\
         fn dup { log `x`; }\n\
         pr p [url = `u` dir = `d`] {\n\
             use dup;\n\
             use dup;\n\
         }\n\
         ",
        );
        let err = compile_full(&dir.path().join("main.kiru")).unwrap_err();
        assert!(
            err.to_string().contains("duplicate function"),
            "got: {}",
            err
        );
    }

    #[test]
    fn test_unknown_global_function_use_errors() {
        let dir = tempfile::TempDir::new().unwrap();
        write_config(
            dir.path(),
            "main.kiru",
            "\
         pr p [url = `u` dir = `d`] { use missing; }\
         ",
        );
        let err = compile_full(&dir.path().join("main.kiru")).unwrap_err();
        assert!(
            err.to_string().contains("unknown global function"),
            "got: {}",
            err
        );
    }

    #[test]
    fn test_use_of_undeclared_function_rejected() {
        let dir = tempfile::TempDir::new().unwrap();
        write_config(
            dir.path(),
            "main.kiru",
            "\
         pr p [url = `u` dir = `d`] { use helper; }\
         ",
        );
        let err = compile_full(&dir.path().join("main.kiru")).unwrap_err();
        assert!(
            err.to_string().contains("unknown global function"),
            "got: {}",
            err
        );
    }

    #[test]
    fn test_global_function_body_may_only_reference_self_or_global() {
        let dir = tempfile::TempDir::new().unwrap();
        write_config(
            dir.path(),
            "main.kiru",
            "\
         pr other [url = `u` dir = `d`] { var string secret = `x`; }\n\
         fn leak { log `${other::secret}`; }\n\
         pr p [url = `u` dir = `d`] { }\
         ",
        );
        let err = compile_full(&dir.path().join("main.kiru")).unwrap_err();
        assert!(
            err.to_string()
                .contains("may only reference `self::` or `global::`"),
            "got: {}",
            err
        );
    }
}
