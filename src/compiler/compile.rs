use crate::compiler::error::{CompileError, io_err, spanned_err_named};
use crate::compiler::fnstmt::validate_fn_body_stmts;

use crate::compiler::namespaces::{Namespaces, resolve_expr};
use crate::compiler::resolve::{reject_field_fn_body_var_refs, resolve_config};
use crate::compiler::types::{ProjectVarStmt, UnresolvedProject};
use crate::dsl::Parser;
use crate::dsl::lexer::Lexer;
use crate::dsl::{Expr, Program, ProjectField, Stmt, TopLevel, VarType, ast::QualifiedFnRef};
use crate::error::SourceFile;
use crate::error::spanned_report;
use crate::plan::Plan;
use crate::shell::execute_shell_variable;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};

/// Run the full compilation pipeline:
/// 1. Linear processing: walk items in source order, resolve globals and load
///    imports (both in source order). A global `var shell` is executed live at
///    its declaration point, so a later `import` path or global that reads
///    `global::name` sees its real output. Projects are accumulated into the
///    single `Namespaces` map (their names declared for reference checks).
/// 2. Validate references against the namespaces map.
/// 3. Resolve in dependency order: run each project/function `var shell`
///    command and inline every value into the plan.
pub fn compile_and_resolve(entry_path: &Path, force_cwd: bool) -> Result<Plan, CompileError> {
    let abs_entry = canonicalize_entry(entry_path)?;
    let linear_result = resolve_linear(&abs_entry, false)?;
    // Globals (including their `var shell` output) are already resolved; project
    // and function `var shell` commands run in the resolve pass below.
    let sources = linear_result.unresolved.source_texts.clone();
    resolve_config(
        linear_result.namespaces,
        linear_result.unresolved,
        &sources,
        force_cwd,
        true,
    )
}

// Linear-processing pipeline

/// Mutable state threaded through the linear processing phase.
struct LinearState {
    /// The single compile-time resolution map, built incrementally so that
    /// `import` paths (and later reference validation) can resolve variable
    /// references as soon as their names are declared.
    namespaces: Namespaces,
    projects: BTreeMap<String, UnresolvedProject>,
    /// Top-level (shared) functions, keyed by function name. A project binds
    /// one of these into itself with `use name;`. The body is stored unmodified;
    /// its `self::` references are rewritten to the destination project when the
    /// binding is instantiated.
    global_functions: BTreeMap<String, Vec<crate::dsl::FnStmt>>,
    /// Top-level `run` blocks, keyed by run name.
    runs: BTreeMap<String, Vec<Vec<QualifiedFnRef>>>,
    loaded_files: HashSet<PathBuf>,
    recursion_stack: HashSet<PathBuf>,
    skip_missing: bool,
    source_texts: HashMap<String, String>,
}

impl LinearState {
    fn new(skip_missing: bool) -> Self {
        Self {
            namespaces: Namespaces::new(),
            projects: BTreeMap::new(),
            global_functions: BTreeMap::new(),
            loaded_files: HashSet::new(),
            recursion_stack: HashSet::new(),
            skip_missing,
            source_texts: HashMap::new(),
            runs: BTreeMap::new(),
        }
    }
}

/// Intermediate result from the linear processing phase.
struct LinearResult {
    unresolved: super::types::UnresolvedConfig,
    namespaces: Namespaces,
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
    std::fs::canonicalize(&abs_path).map_err(|e| io_err("Failed to resolve", &abs_path, &e))
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

/// Walk items in lexical order, resolving vars into the namespaces map, loading
/// imports when their paths become resolvable, and accumulating projects.
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

/// Merge a single statement into a project body during AST collection.
pub(crate) fn merge_project_body_stmt(
    project: &mut UnresolvedProject,
    stmt: &Stmt,
    sources: &HashMap<String, String>,
    source_name: &str,
) -> Result<(), CompileError> {
    let make_err = |msg: String, offset: usize, len: usize| -> CompileError {
        spanned_err_named(msg, sources, source_name, offset, len)
    };
    match stmt {
        Stmt::Var { .. } => {}
        Stmt::Use { .. } => {}
        Stmt::Field {
            key,
            value,
            offset,
            len,
            ..
        } => {
            let slot: &mut Option<Expr> = match &key {
                ProjectField::Url => &mut project.url,
                ProjectField::Dir => &mut project.dir,
                ProjectField::Sync => &mut project.sync,
                ProjectField::Branch => &mut project.branch,
            };
            if slot.is_some() {
                return Err(make_err(
                    format!("duplicate field '{:?}' in project '{}'", key, project.name),
                    *offset,
                    *len,
                ));
            }
            *slot = Some(value.clone());
        }
        _ => {}
    }
    Ok(())
}

/// Process a `pr <name> { ... }` block: set up the project entry, populate
/// fields, collect fn/run/var blocks, and declare the project's variable names
/// into the namespaces map (detecting exact `project::name` duplicates).
fn process_project_block(
    name: &str,
    fields: &[Stmt],
    body: &[Stmt],
    state: &mut LinearState,
    program: &Program,
) -> Result<(), CompileError> {
    // Register the project namespace immediately so references like
    // `name::var` resolve during the validation pass. Real values are filled
    // in by the resolve pass. A project's metadata fields (`url`/`dir`/
    // `sync`/`branch`) are never referenceable, so they are not registered.
    state.namespaces.declare_project(name)?;

    let project_entry =
        state
            .projects
            .entry(name.to_owned())
            .or_insert_with(|| UnresolvedProject {
                name: name.to_owned(),
                source_file: program.source_name.clone(),
                url: None,
                dir: None,
                sync: None,
                branch: None,
                var_stmts: Vec::new(),
                functions: BTreeMap::new(),
            });

    for field_stmt in fields {
        merge_project_body_stmt(
            project_entry,
            field_stmt,
            &state.source_texts,
            &program.source_name,
        )?;
    }

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

            if project_entry.functions.contains_key(&bound_name) {
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
            let mut body = global_body.clone();
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

            // Validate the function body's variable references.
            let mut errors = Vec::new();
            let fn_key = format!("{}::{}", name, bound_name);
            let mut fn_locals = HashMap::new();
            fn_locals.insert(fn_key.clone(), fn_local_names);
            let mut validation_ctx = crate::compiler::fnstmt::ValidateFnCtx {
                fn_name: &bound_name,
                proj_name: name,
                namespaces: &state.namespaces,
                fn_locals: &fn_locals,
                fn_key: &fn_key,
                errors: &mut errors,
                sources: &state.source_texts,
            };
            validate_fn_body_stmts(&body, &mut validation_ctx);
            if !errors.is_empty() {
                return Err(CompileError::ValidationReport(errors));
            }

            project_entry.functions.insert(bound_name, body);
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
            // Declare the body-var name into the project namespace now so a
            // later reference (or a sibling fn) resolves it; the real value is
            // filled in during the resolve pass.
            state.namespaces.declare_project_var(
                name,
                var_name,
                String::new(),
                &program.source_name,
                *offset,
                *len,
                &state.source_texts,
            )?;
            project_entry.var_stmts.push(ProjectVarStmt {
                var_type: var_type.clone(),
                name: var_name.clone(),
                value: value.clone(),
                offset: *offset,
                len: *len,
            });
        }
    }

    // Normalize project metadata fields and body variables so their
    // `self::` references resolve to the enclosing project (and cross-project
    // reads are rejected).
    let mut scope_errors = Vec::new();
    crate::compiler::scope::normalize_project(
        project_entry,
        &state.source_texts,
        &mut scope_errors,
    );
    if !scope_errors.is_empty() {
        return Err(CompileError::ValidationReport(scope_errors));
    }

    Ok(())
}

/// Collects the names of every variable declared inside function bodies
/// (`var`/`var shell`), recursing through `env` and `case` nesting. Used by
/// the inline `use` handler to exclude a function's own local variables from
/// the eager "missing `self::` variable" check.
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

/// Walk a function body (including `env` and `case` nesting) and record every
/// `var` as a local of the binding `fn_key` (`project::function`). A local
/// variable collides with another local in the same function, or with a
/// variable the applying project already declared (so a shared function whose
/// local name clashes with a host project's variable is rejected at the
/// applying `use`). `use_offset`/`use_len`/`use_source` point at that `use`, so
/// the diagnostic names the project that applied the shared function rather than
/// its reusable definition.
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
                namespaces.declare_fn_body_var(project_name, &s.name);
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

/// Resolve and load an `import` statement. The expression is first
/// interpolated (variable substitution), then resolved against the
/// directory of the source file.  `Path::join` handles absolute vs
/// relative transparently.
fn process_import(
    expr: &Expr,
    state: &mut LinearState,
    program: &Program,
) -> Result<(), CompileError> {
    // An import path is a top-level expression, so the `self` alias means
    // `global` and only `self::`/`global::` reads are legal.
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

/// Process a program's items in lexical order.
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
                    // Resolve the global immediately, in source order, and
                    // declare it into the namespaces map so later globals and
                    // `import` paths can read `global::name`. A `var shell`
                    // global is executed live here (not deferred), because an
                    // `import` path may depend on its output and imports are
                    // loaded during this same linear pass. Globals always run in
                    // the current process directory.
                    // Rewrite the `self` alias (which means `global` at the top
                    // level) and reject any project-namespaced read: a global
                    // variable may only reference `self::` or `global::`.
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
                    // A top-level `fn` is a shared (global) function. It is not a
                    // project function; a project binds it with `use` and runs it
                    // with the project's `cwd`. Its body may only reference
                    // `self::` (the future applying project, left symbolic here)
                    // or `global::`, so check that scope rule now — but do NOT
                    // freeze `self::` to `global`. `TEMPLATE_SCOPE` keeps `self::`
                    // unchanged so the binding happens when the function is
                    // `use`d. Duplicate global function names are rejected.
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
                    // A function application (`use`) is only valid inside a
                    // project body, where it is collected into `pending_uses` by
                    // `process_project_block`. At the top level it is a syntax
                    // error.
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
                    // Rewrite `self::` to `global::` (top-level run block).
                    for chain in chains {
                        for reference in chain {
                            if reference.project == "self" {
                                // Can't mutate through shared ref; clone the chains.
                            }
                        }
                    }
                    let mut chains = chains.clone();
                    for chain in &mut chains {
                        for reference in chain {
                            if reference.project == "self" {
                                reference.project = "global".to_string();
                            }
                            // Validate that the referenced project and function exist.
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

/// The core linear processing phase: entry point.
fn resolve_linear(entry_path: &Path, skip_missing: bool) -> Result<LinearResult, CompileError> {
    let mut state = LinearState::new(skip_missing);
    linear_process_file(entry_path, &mut state)?;

    // Reject metadata field references to function-body variables now that
    // every function has been instantiated (the declare pass ran inline during
    // `use` processing).
    for project in state.projects.values() {
        reject_field_fn_body_var_refs(&project.dir, "dir", &state.namespaces, &state.source_texts)?;
        reject_field_fn_body_var_refs(&project.url, "url", &state.namespaces, &state.source_texts)?;
        reject_field_fn_body_var_refs(
            &project.sync,
            "sync",
            &state.namespaces,
            &state.source_texts,
        )?;
        reject_field_fn_body_var_refs(
            &project.branch,
            "branch",
            &state.namespaces,
            &state.source_texts,
        )?;
    }

    let unresolved = super::types::UnresolvedConfig {
        projects: state.projects,
        runs: std::mem::take(&mut state.runs),
        source_texts: state.source_texts,
    };

    Ok(LinearResult {
        unresolved,
        namespaces: state.namespaces,
    })
}

/// Lightweight compilation that resolves project metadata without validating
/// or lowering function bodies. Used by `kiru sync`, which only needs each
/// project's `url`/`dir`/`sync`/`branch`. Reuses the same resolve pass with
/// function lowering disabled and import resolution tolerant of not-yet-cloned
/// repositories (`SkipMissing`).
pub fn parse_projects_metadata(entry_path: &Path) -> Result<Plan, CompileError> {
    let abs_entry = canonicalize_entry(entry_path)?;
    let linear = resolve_linear(&abs_entry, true)?;
    let sources = linear.unresolved.source_texts.clone();
    resolve_config(linear.namespaces, linear.unresolved, &sources, false, false)
}

#[cfg(test)]
mod tests {
    use crate::compiler::test_support::*;
    use crate::compiler::{CompileError, parse_projects_metadata};

    /// Regression test for the cross-file wrong-location bug: when a project
    /// body is merged from several `.kiru` files (all declaring `pr kiru`), a
    /// redeclaration diagnostic must point at the file that actually declared
    /// the conflicting node — not the first file that happened to declare
    /// `pr kiru`. Previously the span resolved against `run.kiru` (the first
    /// `pr kiru`) even though the duplicate `docker_bin` lives in `build.kiru`.
    #[test]
    fn test_cross_file_redeclaration_points_at_defining_file() {
        let dir = tempfile::TempDir::new().unwrap();
        // main.kiru imports two sibling files; both declare `pr kiru`.
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
        // The conflicting declaration is in build.kiru, so the span must
        // reference that file — never run.kiru (the first `pr kiru`).
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

    /// Regression: an `import` path that interpolates a value derived from a
    /// global `var shell` must resolve against the command's real output. The
    /// shell global is executed live during the linear pass (in source order),
    /// so a later import sees the true directory rather than an empty
    /// placeholder (which previously produced paths like `/kiru/...`).
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
        // main.kiru imports ./missing.kiru which does not exist.
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
        // The imported file exists and is parseable, so SkipMissing still
        // processes it normally — syntax errors inside must surface.
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
        // Parse errors are wrapped in ParseReports, not silently swallowed.
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
        // The binding is a real project function, callable via `project::shared`.
        assert!(cfg.runs.is_empty());
    }

    #[test]
    fn test_use_function_runs_with_project_cwd() {
        // A shared function runs inside the destination project: its `global::`
        // references resolve against the global scope, and (at runtime) its
        // commands execute with the project's `dir`. `self::` inside the shared
        // body means the applying project, so a shared function may depend on
        // globals as well as the destination project's variables.
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
