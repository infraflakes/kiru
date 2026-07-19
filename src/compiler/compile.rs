use crate::compiler::error::{CompileError, io_err, spanned_err_named};

use crate::compiler::namespaces::{Namespaces, ShellCache, resolve_expr};
use crate::compiler::resolve::resolve_config;
use crate::compiler::types::{ProjectVarStmt, UnresolvedProject};
use crate::compiler::validation;
use crate::dsl::Parser;
use crate::dsl::{Expr, Program, ProjectField, Stmt, TopLevel, VarType};
use crate::plan::Plan;
use miette::miette;
use std::collections::HashMap;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// Run the full compilation pipeline:
/// 1. Linear processing: walk items in source order, resolve vars and fields,
///    load imports (with variable interpolation), accumulate projects, and
///    build the single `Namespaces` map (variable names declared, `var shell`
///    globals left as placeholders to be evaluated later).
/// 2. Validate references against the namespaces map (no shell runs yet).
/// 3. Resolve in dependency order: run `var shell` commands and inline every
///    value into the plan.
pub fn compile_and_resolve(entry_path: &Path, force_cwd: bool) -> Result<Plan, CompileError> {
    let abs_entry = canonicalize_entry(entry_path)?;
    let linear_result = resolve_linear(&abs_entry, ImportPolicy::Strict)?;
    // Validation runs before any shell execution: the namespaces map already
    // carries every declared variable name, so reference checks need no
    // command output.
    let sources = linear_result.unresolved.source_texts.clone();
    validation::validate_configuration(
        &linear_result.unresolved,
        &linear_result.namespaces,
        &sources,
    )?;
    let mut shell_cache = ShellCache::new();
    resolve_config(
        linear_result.namespaces,
        linear_result.unresolved,
        &sources,
        force_cwd,
        &mut shell_cache,
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
    /// Top-level `var` / `var shell` declarations, in source order.
    global_vars: Vec<ProjectVarStmt>,
    projects: HashMap<String, UnresolvedProject>,
    loaded_files: HashSet<PathBuf>,
    recursion_stack: HashSet<PathBuf>,
    import_policy: ImportPolicy,
    source_texts: HashMap<String, String>,
}

impl LinearState {
    fn new(import_policy: ImportPolicy) -> Self {
        Self {
            namespaces: Namespaces::new(),
            global_vars: Vec::new(),
            projects: HashMap::new(),
            loaded_files: HashSet::new(),
            recursion_stack: HashSet::new(),
            import_policy,
            source_texts: HashMap::new(),
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

/// Walk items in lexical order, resolving vars into the namespaces map, loading
/// imports when their paths become resolvable, and accumulating projects.
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
            project.fn_order.push(name.clone());
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
/// fields, collect fn/run/var blocks, and declare the project's variable names
/// into the namespaces map (detecting exact `project::name` duplicates).
fn process_project_block(
    name: &str,
    fields: &[Stmt],
    body: &[Stmt],
    offset: usize,
    len: usize,
    state: &mut LinearState,
    program: &Program,
) -> Result<(), CompileError> {
    // Register the project namespace immediately so references like
    // `name::var` resolve during the validation pass. Real values are filled
    // in by the resolve pass. A project's metadata fields (`url`/`dir`/
    // `sync`/`branch`) are never referenceable, so they are not registered.
    state.namespaces.declare_project(
        name,
        &program.source_name,
        offset,
        len,
        &state.source_texts,
    )?;

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
                functions: HashMap::new(),
                fn_order: Vec::new(),
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
        merge_project_body_stmt(
            project_entry,
            body_stmt,
            &state.source_texts,
            &program.source_name,
        )?;
    }

    Ok(())
}

/// Walk a function body (including `env` and `case` nesting) and declare every
/// `var` into the project namespace, erroring on an exact duplicate
/// `project::name`. Mirrors the redeclaration rule applied to project-body vars.
fn declare_fn_body_vars(
    namespaces: &mut Namespaces,
    project_name: &str,
    stmts: &[crate::dsl::FnStmt],
    source_texts: &HashMap<String, String>,
) -> Result<(), CompileError> {
    for stmt in stmts {
        match stmt {
            crate::dsl::FnStmt::VarDecl(s) => {
                let (offset, len) = s.value.offset_len();
                namespaces.declare_project_var(
                    project_name,
                    &s.name,
                    String::new(),
                    s.value.source_name(),
                    offset,
                    len,
                    source_texts,
                )?;
                namespaces.declare_fn_body_var(project_name, &s.name);
            }
            crate::dsl::FnStmt::EnvBlock(s) => {
                declare_fn_body_vars(namespaces, project_name, &s.body, source_texts)?;
            }
            crate::dsl::FnStmt::Case(s) => {
                for arm in &s.scopes {
                    declare_fn_body_vars(namespaces, project_name, &arm.body, source_texts)?;
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
    let path_str = resolve_expr(expr, &state.namespaces, &state.source_texts)?;
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
                Stmt::Var {
                    var_type,
                    name,
                    value,
                    offset,
                    len,
                    ..
                } => {
                    // Collect the global declaration for the resolve pass and
                    // declare its name into the namespaces map immediately so
                    // later globals / imports can reference it. `var shell`
                    // globals are left as a placeholder; their command output is
                    // evaluated in the resolve pass.
                    let resolved = resolve_expr(value, &state.namespaces, &state.source_texts)?;
                    let placeholder = if *var_type == VarType::Shell {
                        String::new()
                    } else {
                        resolved
                    };
                    state.namespaces.declare_global(
                        name,
                        placeholder,
                        &program.source_name,
                        *offset,
                        *len,
                        &state.source_texts,
                    )?;
                    state.global_vars.push(ProjectVarStmt {
                        var_type: var_type.clone(),
                        name: name.clone(),
                        value: value.clone(),
                        offset: *offset,
                        len: *len,
                    });
                }
                Stmt::Project {
                    name,
                    fields,
                    body,
                    offset,
                    len,
                    ..
                } => {
                    process_project_block(name, fields, body, *offset, *len, state, program)?;
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

    // Declare function-body variables into their project namespaces now that
    // every file has been merged. Walking the merged project map exactly once
    // (rather than per-file) avoids double-counting vars that live inside
    // functions merged from several `.kiru` files. This makes cross-references
    // and exact-duplicate redeclarations visible before the resolve pass.
    for (project_name, project) in &state.projects {
        for fn_name in &project.fn_order {
            let fn_body = &project.functions[fn_name];
            declare_fn_body_vars(
                &mut state.namespaces,
                project_name,
                fn_body,
                &state.source_texts,
            )?;
        }
    }

    let unresolved = super::types::UnresolvedConfig {
        global_vars: std::mem::take(&mut state.global_vars),
        projects: state.projects,
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
    let linear = resolve_linear(&abs_entry, ImportPolicy::SkipMissing)?;
    let sources = linear.unresolved.source_texts.clone();
    let mut shell_cache = ShellCache::new();
    resolve_config(
        linear.namespaces,
        linear.unresolved,
        &sources,
        false,
        &mut shell_cache,
        false,
    )
}

#[cfg(test)]
mod tests {
    use crate::compiler::test_support::*;
    use crate::compiler::{CompileError, parse_projects_metadata};
    use crate::dsl::ast::QualifiedFnRef;

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
             run release { test::build; }\n\
             run ci { test::build; }\n\
         }\
         ",
        );
        let cfg = compile_full(&dir.path().join("main.kiru")).unwrap();
        let proj = &cfg.projects["test"];
        assert!(proj.functions.contains_key("build"));
        assert!(proj.runs.contains_key("release"));
        assert!(proj.runs.contains_key("ci"));
        assert_eq!(
            proj.runs["release"],
            vec![vec![QualifiedFnRef {
                project: "test".to_string(),
                function: "build".to_string()
            }]]
        );
        assert_eq!(
            proj.runs["ci"],
            vec![vec![QualifiedFnRef {
                project: "test".to_string(),
                function: "build".to_string()
            }]]
        );
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
             run all { p::build => p::test; }\n\
             run ci { p::build; }\n\
         }\
         ",
        );
        let cfg = compile_full(&dir.path().join("main.kiru")).unwrap();
        let proj = &cfg.projects["p"];
        assert!(proj.runs.contains_key("all"));
        assert!(proj.runs.contains_key("ci"));
        assert_eq!(proj.runs.len(), 2);
        assert_eq!(
            proj.runs["all"],
            vec![vec![
                QualifiedFnRef {
                    project: "p".to_string(),
                    function: "build".to_string()
                },
                QualifiedFnRef {
                    project: "p".to_string(),
                    function: "test".to_string()
                }
            ]]
        );
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
             run dup { test::check; }\n\
             run dup { test::check; }\n\
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
             run dup { p::x; }\n\
             run dup { p::x; }\n\
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
