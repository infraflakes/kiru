use crate::compiler::error::{CompileError, io_err, spanned_err_named};

use crate::compiler::resolve::{self, PendingShell, ShellCache, config_eval_top_level};
use crate::compiler::scope::ScopeStack;
use crate::compiler::types::{ProjectVarStmt, UnresolvedProject};
use crate::compiler::validation;
use crate::dsl::Parser;
use crate::dsl::{Expr, Program, ProjectField, Stmt, TopLevel};
use crate::plan::{Plan, PlanProject};
use miette::miette;
use std::collections::HashMap;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// Run the full compilation pipeline:
/// 1. Linear processing: walk items in source order, resolve vars and fields,
///    load imports with variable interpolation, accumulate projects.
/// 2. Validate using the resolved state.
/// 3. Fully resolve function bodies against the flat var scope.
pub fn compile_and_resolve(entry_path: &Path, force_cwd: bool) -> Result<Plan, CompileError> {
    let abs_entry = canonicalize_entry(entry_path)?;
    let linear_result = resolve_linear(&abs_entry, ImportPolicy::Strict)?;
    let source_texts = linear_result.unresolved.source_texts.clone();
    // Validation runs before any shell execution: top-level `var shell` vars
    // are declared as placeholders during linear processing, so validation
    // sees their names without running commands.
    validation::validate_configuration(&linear_result.unresolved, &linear_result.var_scope)?;
    // Config-eval phase: the ONLY place `var shell` commands run. Memoized so
    // identical commands evaluate at most once per invocation.
    let mut shell_cache = ShellCache::new();
    let mut var_scope = linear_result.var_scope;
    config_eval_top_level(
        linear_result.pending_shell,
        &mut var_scope,
        &mut shell_cache,
        &source_texts,
    )?;
    resolve::resolve_with_scopes(
        linear_result.unresolved,
        var_scope,
        &source_texts,
        force_cwd,
        &mut shell_cache,
    )
}

// Linear-processing pipeline

/// Mutable state threaded through the linear processing phase.
struct LinearState {
    var_scope: ScopeStack<String>,
    projects: HashMap<String, UnresolvedProject>,
    loaded_files: HashSet<PathBuf>,
    recursion_stack: HashSet<PathBuf>,
    import_policy: ImportPolicy,
    source_texts: HashMap<String, String>,
    /// Top-level `var shell` declarations deferred to the post-validation
    /// config-eval phase (see `config_eval_top_level`).
    pending_shell: Vec<PendingShell>,
}

impl LinearState {
    fn new(import_policy: ImportPolicy) -> Self {
        Self {
            var_scope: ScopeStack::new(),
            projects: HashMap::new(),
            loaded_files: HashSet::new(),
            recursion_stack: HashSet::new(),
            import_policy,
            source_texts: HashMap::new(),
            pending_shell: Vec::new(),
        }
    }
}

/// Intermediate result from the linear processing phase.
struct LinearResult {
    unresolved: super::types::UnresolvedConfig,
    var_scope: ScopeStack<String>,
    pending_shell: Vec<PendingShell>,
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
    let mut parser = Parser::from_source(data).with_source_name(source_name.clone());
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
    let canon_path =
        std::fs::canonicalize(file_path).map_err(|e| io_err("Failed to resolve", file_path, &e))?;

    if state.recursion_stack.contains(&canon_path) {
        return Err(CompileError::ValidationReport(vec![miette!(
            "circular import: {}",
            canon_path.display()
        )]));
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

/// Policy for handling `import` statements whose target file does not exist.
/// Used by [`resolve_linear`] to control whether a missing import is a hard
/// error (`Strict`) or silently skipped with a warning (`SkipMissing`).
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ImportPolicy {
    /// Missing import files are hard errors.
    Strict,
    /// Missing import files are skipped with a warning; the walk continues.
    SkipMissing,
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
        Stmt::Field {
            key,
            value,
            offset,
            len,
            ..
        } => {
            // One arm per project field: detect duplicates and store the
            // parsed `Expr`. Compiler-enforced exhaustiveness keeps this in
            // sync with `ProjectField`.
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
            return Err(spanned_err_named(
                format!(
                    "unexpected statement in project '{}' (only var, fn, and run are valid)",
                    project.name
                ),
                sources,
                source_name,
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
                url: None,
                dir: None,
                sync: None,
                branch: None,
                declared_var_names: HashSet::new(),
                var_stmts: Vec::new(),
                functions: HashMap::new(),
                runs: HashMap::new(),
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
        if let Stmt::Var {
            var_type,
            name,
            value,
            offset,
            len,
            ..
        } = body_stmt
        {
            // Record the declared body-var name so validation can seed it and
            // function bodies may reference it. Duplicate detection itself is
            // performed during resolution (resolve_with_scopes), where the
            // var is declared into the project frame.
            project_entry.declared_var_names.insert(name.clone());
            project_entry.var_stmts.push(ProjectVarStmt {
                var_type: var_type.clone(),
                name: name.clone(),
                value: value.clone(),
                offset: *offset,
                len: *len,
            });
        }
        merge_project_body_stmt(
            project_entry,
            body_stmt,
            &state.source_texts,
            &program.source_name,
        )?;
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
    let path_str = resolve::resolve_expr(expr, &state.var_scope, &state.source_texts)?;
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
    let base_dir = Path::new(&program.source_name).parent().ok_or_else(|| {
        CompileError::ValidationReport(vec![miette!(
            "cannot determine base directory for import from '{}'",
            program.source_name
        )])
    })?;
    let target = base_dir.join(&path_str);

    if state.import_policy == ImportPolicy::SkipMissing && !target.exists() {
        eprintln!(
            "{:?}",
            miette::miette!(
                "import target '{}' does not exist yet (from {}), skipping",
                path_str,
                program.source_name
            )
        );
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
                Stmt::Var { .. } => {
                    // Collect (don't execute): `var string` is declared now;
                    // `var shell` is declared as a placeholder and deferred to
                    // the config-eval phase after validation.
                    if let Some(pending) = resolve::collect_top_level_var(
                        stmt,
                        &mut state.var_scope,
                        &state.source_texts,
                    )? {
                        state.pending_shell.push(pending);
                    }
                }
                Stmt::Project {
                    name, fields, body, ..
                } => {
                    process_project_block(name, fields, body, state, program)?;
                }
                Stmt::Fn { offset, len, .. } | Stmt::Run { offset, len, .. } => {
                    return Err(spanned_err_named(
                        format!("unexpected statement in '{}'", program.source_name),
                        &state.source_texts,
                        &program.source_name,
                        *offset,
                        *len,
                    ));
                }
                Stmt::Field {
                    key, offset, len, ..
                } => {
                    return Err(spanned_err_named(
                        format!(
                            "field '{:?}' is not inside a project block in '{}'",
                            key, program.source_name
                        ),
                        &state.source_texts,
                        &program.source_name,
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
fn resolve_linear(
    entry_path: &Path,
    import_policy: ImportPolicy,
) -> Result<LinearResult, CompileError> {
    let mut state = LinearState::new(import_policy);
    linear_process_file(entry_path, &mut state)?;

    let unresolved = super::types::UnresolvedConfig {
        projects: state.projects,
        source_texts: state.source_texts,
    };

    Ok(LinearResult {
        unresolved,
        var_scope: state.var_scope,
        pending_shell: state.pending_shell,
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
/// The returned [`Plan`] has empty function maps — function and run
/// blocks are collected during linear processing but never resolved.
pub fn parse_projects_metadata(entry_path: &Path) -> Result<Plan, CompileError> {
    let abs_entry = canonicalize_entry(entry_path)?;
    let linear = resolve_linear(&abs_entry, ImportPolicy::SkipMissing)?;

    let mut projects = HashMap::new();
    let source_texts = &linear.unresolved.source_texts;
    // Config-eval phase: evaluate top-level `var shell` so project fields can
    // reference their outputs. Shares the same funnel as `compile_and_resolve`.
    let mut shell_cache = ShellCache::new();
    let mut global_scope = linear.var_scope;
    config_eval_top_level(
        linear.pending_shell,
        &mut global_scope,
        &mut shell_cache,
        source_texts,
    )?;
    for (name, unresolved_project) in linear.unresolved.projects {
        // Fields reference global vars only; project body vars are not visible
        // to fields, so no project frame is needed here.
        let mut scope = ScopeStack::new();
        scope.seed_global(
            global_scope
                .iter_global()
                .map(|(k, v)| (k.clone(), v.clone())),
        );

        let (url, dir, sync, branch) =
            resolve::resolve_project_fields(&unresolved_project, &scope, source_texts)?;

        projects.insert(
            name,
            PlanProject {
                url,
                dir,
                sync,
                branch,
                functions: HashMap::new(),
                runs: unresolved_project.runs,
            },
        );
    }

    Ok(Plan { projects })
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
            "pr kiru { fn all { log `hi`; } }\n",
        );
        write_config(
            &dir.path().join(".kiru"),
            "build.kiru",
            "\
            pr kiru {\n\
                var string docker_bin = `docker`;\n\
                fn build_with_container {\n\
                    var string docker_bin = `docker`;\n\
                }\n\
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
        pr test [\n\
            url = `http://example.com`\n\
            dir = `test`\n\
        ] {\n\
            var string app = `todo`;\n\
            fn build { log `hi`; }\n\
            run release { build; }\n\
            run ci { build; }\n\
        }\
        ",
        );
        let cfg = compile_full(&dir.path().join("main.kiru")).unwrap();
        let proj = &cfg.projects["test"];
        assert!(proj.functions.contains_key("build"));
        assert!(proj.runs.contains_key("release"));
        assert!(proj.runs.contains_key("ci"));
        assert_eq!(proj.runs["release"], vec![vec!["build"]]);
        assert_eq!(proj.runs["ci"], vec![vec!["build"]]);
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
        pr p [url = $extra dir = `d`] { }
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
        pr p1 [url = `u` dir = `d1`] { }\n\
        pr p1 { fn build { log `x`; } }\
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
        pr p [url = $a dir = `d`] { }\
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
        pr p [ url = `http://x` dir = `x` ] {\n\
            fn build { log `building`; }\n\
            fn test { exec `check`; }\n\
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
        pr p [ url = `http://x` dir = `x` ] {\n\
            fn build { log `x`; }\n\
            fn test { log `y`; }\n\
            run all { build => test; }\n\
            run ci { build; }\n\
        }\
        ",
        );
        let cfg = compile_full(&dir.path().join("main.kiru")).unwrap();
        let proj = &cfg.projects["p"];
        assert!(proj.runs.contains_key("all"));
        assert!(proj.runs.contains_key("ci"));
        assert_eq!(proj.runs.len(), 2);
        assert_eq!(proj.runs["all"], vec![vec!["build", "test"]]);
    }

    #[test]
    fn test_duplicate_fn_in_project() {
        let dir = tempfile::TempDir::new().unwrap();
        write_config(
            dir.path(),
            "main.kiru",
            "\
        pr test [\n\
            url = `u`\n\
            dir = `d`\n\
        ] {\n\
            fn dup { log `a`; }\n\
            fn dup { log `b`; }\n\
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
        pr test [\n\
            url = `u`\n\
            dir = `d`\n\
        ] {\n\
            fn check { log `x`; }\n\
            run dup { check; }\n\
            run dup { check; }\n\
        }\
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
        pr p [ url = `http://x` dir = `x` ] {\n\
            fn dup { log `a`; }\n\
            fn dup { log `b`; }\n\
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
        pr p [ url = `http://x` dir = `x` ] {\n\
            fn x { log `a`; }\n\
            run dup { x; }\n\
            run dup { x; }\n\
        }\
        ",
        );
        let err = compile_full(&dir.path().join("main.kiru")).unwrap_err();
        assert!(err.to_string().contains("duplicate run"), "got: {}", err);
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
}
