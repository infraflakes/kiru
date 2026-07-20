use crate::compiler::error::CompileError;
use crate::compiler::error::spanned_err_named;
use crate::compiler::error::spanned_err_on_field;
use crate::compiler::fnstmt::resolve_fn_body_stmts;
use crate::compiler::namespaces::{
    Namespaces, resolve_dir_field, resolve_expr, resolve_optional_expr,
};
use crate::compiler::types::{ProjectVarStmt, UnresolvedConfig};
use crate::dsl::Expr;
use crate::dsl::VarType;
use crate::error::SourceFile;
use crate::plan::{Plan, PlanProject, parse_sync_mode};
use crate::shell::execute_shell_variable;
use std::collections::HashMap;
use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// A project's metadata field (`url`/`dir`/`sync`/`branch`) may reference
/// config variables (globals and this project's own body variables) but never a
/// function-body variable. Reject such a reference before resolution.
pub(crate) fn reject_field_fn_body_var_refs(
    field: &Option<Expr>,
    field_kind: &str,
    namespaces: &Namespaces,
    sources: &HashMap<String, String>,
) -> Result<(), CompileError> {
    let Some(expr) = field else {
        return Ok(());
    };
    let mut bad: Option<(String, String)> = None;
    expr.visit_vars(&mut |name: &str, ns: &str| {
        if namespaces.is_fn_body_var(ns, name) {
            bad = Some((ns.to_string(), name.to_string()));
        }
    });
    if let Some((ns, name)) = bad {
        return Err(spanned_err_on_field(
            format!(
                "field {} cannot reference function-body variable {}::{}",
                field_kind, ns, name
            ),
            sources,
            field,
            field_kind,
        ));
    }
    Ok(())
}

/// Resolve using the single namespaces map.
///
/// Projects are isolated: a variable reference may only read `self::` (the
/// project itself) or `global::`, never another project, so projects have no
/// inter-dependencies and may be resolved in any order. They are resolved in
/// sorted-name order purely for deterministic diagnostics (e.g. which
/// duplicate-directory pair is reported first). `lower_functions` controls
/// whether function bodies are lowered into `PlanStmt`s; `kiru sync` sets it to
/// `false` (it only needs the project metadata). `force_cwd` mirrors the
/// `KIRU_CWD` env var: when set, project-body `var shell` commands run in the
/// current directory instead of the resolved project directory.
///
/// Globals were already fully resolved (and their `var shell` commands executed)
/// during the linear pass, so `namespaces` already carries their real values;
/// this pass only resolves per-project variables, fields, and function bodies.
pub(crate) fn resolve_config(
    mut namespaces: Namespaces,
    unresolved: UnresolvedConfig,
    sources: &std::collections::HashMap<String, String>,
    force_cwd: bool,
    lower_functions: bool,
) -> Result<Plan, CompileError> {
    let mut projects = std::collections::BTreeMap::new();
    let mut seen_dirs: HashSet<String> = HashSet::new();

    for name in unresolved.projects.keys() {
        let unresolved_project = &unresolved.projects[name];

        // `dir` is needed to compute the working directory for `var shell` and
        // to detect duplicate directories. It may reference globals (this
        // project's own body variables are not yet resolved). A project's
        // metadata fields are internal runner data and are never themselves
        // referenceable, and may never read a function-body variable.
        reject_field_fn_body_var_refs(&unresolved_project.dir, "dir", &namespaces, sources)?;
        let dir = resolve_dir_field(unresolved_project, &namespaces, sources)?;

        if !dir.is_empty() && !seen_dirs.insert(dir.clone()) {
            return Err(spanned_err_named(
                format!("project {:?}: duplicate directory {:?}", name, dir),
                sources,
                &unresolved_project.source_file,
                0,
                1,
            ));
        }

        // The project body runs in the resolved project directory (or the
        // current directory when force_cwd is set / dir is empty).
        let effective_dir: Option<PathBuf> = if force_cwd || dir.is_empty() {
            None
        } else {
            Some(PathBuf::from(&dir))
        };
        let working_dir: Option<&Path> = effective_dir.as_deref();

        // Body variables resolve first so the remaining field expressions
        // (`url`/`sync`/`branch`) may read this project's own config
        // variables. A body var or function may read only `self::` (this
        // project) or `global::`; fields are not referenceable.
        for var_stmt in &unresolved_project.var_stmts {
            resolve_project_var(var_stmt, &mut namespaces, name, working_dir, sources)?;
        }

        reject_field_fn_body_var_refs(&unresolved_project.url, "url", &namespaces, sources)?;
        let url = resolve_optional_expr(&unresolved_project.url, &namespaces, sources)?
            .unwrap_or_default();
        reject_field_fn_body_var_refs(&unresolved_project.sync, "sync", &namespaces, sources)?;
        let sync = match resolve_optional_expr(&unresolved_project.sync, &namespaces, sources)? {
            Some(mode) => mode,
            None => "clone".to_string(),
        };
        reject_field_fn_body_var_refs(&unresolved_project.branch, "branch", &namespaces, sources)?;
        let branch = resolve_optional_expr(&unresolved_project.branch, &namespaces, sources)?;

        // Function bodies, in alphabetical order so a later function can read an
        // earlier function's variables deterministically.
        let mut functions = std::collections::BTreeMap::new();
        if lower_functions {
            for fn_name in unresolved_project.functions.keys() {
                let body = &unresolved_project.functions[fn_name];
                let resolved_body =
                    resolve_fn_body_stmts(body, &mut namespaces, name, working_dir, sources)?;
                functions.insert(fn_name.clone(), resolved_body);
            }
        }

        let sync_mode = parse_sync_mode(&sync).map_err(|msg| {
            crate::compiler::error::spanned_err_on_field(
                msg,
                sources,
                &unresolved_project.sync,
                &unresolved_project.source_file,
            )
        })?;

        projects.insert(
            name.clone(),
            PlanProject {
                url,
                dir,
                sync: sync_mode,
                branch,
                functions,
            },
        );
    }

    Ok(Plan {
        projects,
        runs: unresolved.runs,
    })
}

/// Resolve a `var` / `var shell` from a `ProjectVarStmt` into the enclosing
/// project namespace. Runs `var shell` in `working_dir` (executing the command
/// live, every time) and records the real value via
/// `namespaces.set_project_var`.
pub(crate) fn resolve_project_var(
    var: &ProjectVarStmt,
    namespaces: &mut Namespaces,
    project: &str,
    working_dir: Option<&Path>,
    sources: &std::collections::HashMap<String, String>,
) -> Result<(), CompileError> {
    let source = SourceFile::from_registry(sources, var.value.source_name());
    let resolved = resolve_expr(&var.value, namespaces, sources)?;
    let final_val = if var.var_type == VarType::Shell {
        execute_shell_variable(
            &var.name,
            &resolved,
            working_dir,
            &source,
            var.offset,
            var.len,
        )?
    } else {
        resolved
    };
    namespaces.set_project_var(project, &var.name, final_val);
    Ok(())
}

#[cfg(test)]
mod tests {
    use crate::compiler::CompileError;
    use crate::compiler::compile::compile_and_resolve;
    use crate::compiler::test_support::*;
    use crate::plan::PlanStmt;
    use miette::Report;

    #[test]
    fn test_variable_chain_resolution() {
        let dir = tempfile::TempDir::new().unwrap();
        write_config(
            dir.path(),
            "main.kiru",
            "\
         var string a = `x`;\n\
         var string b = $global::a;\n\
         var string c = $global::b;\n\
         pr p [url = $global::c dir = `d`] { }
         ",
        );
        let cfg = compile_full(&dir.path().join("main.kiru")).unwrap();
        assert_eq!(cfg.projects["p"].url, "x");
    }

    #[test]
    fn test_interpolation_in_backtick() {
        let dir = tempfile::TempDir::new().unwrap();
        write_config(
            dir.path(),
            "main.kiru",
            "\
         var string name = `world`;\n\
         pr p [url = `http://${global::name}.com` dir = `d`] { }\
         ",
        );
        let cfg = compile_full(&dir.path().join("main.kiru")).unwrap();
        assert_eq!(cfg.projects["p"].url, "http://world.com");
    }

    #[test]
    fn test_dir_field_resolves_relative_to_defining_file() {
        let dir = tempfile::TempDir::new().unwrap();
        std::fs::create_dir_all(dir.path().join("sub")).unwrap();
        write_config(
            dir.path(),
            "main.kiru",
            "pr x [url = `u`] { }\n\
             import `sub/build.kiru`;\n",
        );
        write_config(
            &dir.path().join("sub"),
            "build.kiru",
            "pr x [dir = `./overridden`] { }",
        );
        let cfg = compile_full(&dir.path().join("main.kiru")).unwrap();
        let expected = dir
            .path()
            .join("sub")
            .join("./overridden")
            .to_string_lossy()
            .to_string();
        assert_eq!(cfg.projects["x"].dir, expected);
    }

    #[test]
    fn test_project_field_with_var_ref() {
        let dir = tempfile::TempDir::new().unwrap();
        write_config(
            dir.path(),
            "main.kiru",
            "\
         var string myurl = `http://example.com`;\n\
         pr x [url = $global::myurl dir = `d`] { }\
         ",
        );
        let cfg = compile_full(&dir.path().join("main.kiru")).unwrap();
        assert_eq!(cfg.projects["x"].url, "http://example.com");
    }

    #[test]
    fn test_project_var_chain_resolution() {
        let dir = tempfile::TempDir::new().unwrap();
        write_config(
            dir.path(),
            "main.kiru",
            "\
         pr test [\n\
             url = `u`\n\
             dir = `d`\n\
         ] {\n\
             var string a = `hello`;\n\
             var string b = $test::a;\n\
         }\
         ",
        );
        let cfg = compile_full(&dir.path().join("main.kiru")).unwrap();
        assert_eq!(
            cfg.projects["test"].dir,
            dir.path().join("d").to_string_lossy()
        );
    }

    #[test]
    fn test_duplicate_dir() {
        let dir = tempfile::TempDir::new().unwrap();
        write_config(
            dir.path(),
            "main.kiru",
            "\
         pr a [url = `ua` dir = `shared`] { }\n\
         pr b [url = `ub` dir = `shared`] { }\
         ",
        );
        let err = compile_full(&dir.path().join("main.kiru")).unwrap_err();
        assert!(
            err.to_string().contains("duplicate directory"),
            "got: {}",
            err
        );
    }

    #[test]
    fn test_invalid_sync_value() {
        let dir = tempfile::TempDir::new().unwrap();
        write_config(
            dir.path(),
            "main.kiru",
            "\
         pr p [url = `u` dir = `d` sync = `invalid`] { }\
         ",
        );
        let err = compile_full(&dir.path().join("main.kiru")).unwrap_err();
        assert!(err.to_string().contains("sync"), "got: {}", err);
    }

    #[test]
    fn test_project_field_can_reference_body_var() {
        let dir = tempfile::TempDir::new().unwrap();
        write_config(
            dir.path(),
            "main.kiru",
            "\
         pr test [\n\
             url = `http://example.com/${test::name}`\n\
             dir = $test::name\n\
         ] {\n\
             var string name = `myproject`;\n\
         }\
         ",
        );
        let cfg = compile_full(&dir.path().join("main.kiru")).unwrap();
        assert_eq!(cfg.projects["test"].url, "http://example.com/myproject");
    }

    #[test]
    fn test_kiru_cwd_forces_current_dir_for_project_scope_var_shell() {
        let dir = tempfile::TempDir::new().unwrap();
        let subdir = dir.path().join("projectdir");
        std::fs::create_dir(&subdir).unwrap();
        let current_dir = std::env::current_dir().unwrap();

        write_config(
            dir.path(),
            "main.kiru",
            &format!(
                "\
         fn check_kcwd {{\n\
             log $self::cwd;\n\
         }}\n\
         pr test [\n\
             url = `http://example.com`\n\
             dir = `{}`\n\
         ] {{\n\
             var shell cwd = `pwd`;\n\
             use check_kcwd as check;\n\
         }}\n\
         ",
                subdir.to_string_lossy()
            ),
        );

        let cfg = compile_and_resolve(&dir.path().join("main.kiru"), true).unwrap();

        let proj = &cfg.projects["test"];
        let fn_body = &proj.functions["check"];
        assert_eq!(fn_body.len(), 1);
        let stmt = match &fn_body[0] {
            PlanStmt::Log(s) => s,
            other => panic!("expected Log statement, got {:?}", other),
        };
        let expected = current_dir.to_string_lossy().to_string();
        assert_eq!(**stmt, expected);
    }

    #[test]
    fn test_project_scope_var_shell_uses_project_dir() {
        let dir = tempfile::TempDir::new().unwrap();
        let subdir = dir.path().join("myproject");
        std::fs::create_dir(&subdir).unwrap();

        write_config(
            dir.path(),
            "main.kiru",
            &format!(
                "\
         fn check_psvs {{\n\
             log $self::cwd;\n\
         }}\n\
         pr test [\n\
             url = `http://example.com`\n\
             dir = `{}`\n\
         ] {{\n\
             var shell cwd = `pwd`;\n\
             use check_psvs as check;\n\
         }}\n\
         ",
                subdir.to_string_lossy()
            ),
        );
        let cfg = compile_full(&dir.path().join("main.kiru")).unwrap();
        let proj = &cfg.projects["test"];
        let fn_body = &proj.functions["check"];
        assert_eq!(fn_body.len(), 1);
        let stmt = match &fn_body[0] {
            PlanStmt::Log(s) => s,
            other => panic!("expected Log statement, got {:?}", other),
        };
        let expected = std::fs::canonicalize(&subdir)
            .unwrap()
            .to_string_lossy()
            .to_string();
        assert_eq!(**stmt, expected);
    }

    #[test]
    fn test_fn_scope_var_shell_uses_project_dir() {
        let dir = tempfile::TempDir::new().unwrap();
        let subdir = dir.path().join("myproject");
        std::fs::create_dir(&subdir).unwrap();

        write_config(
            dir.path(),
            "main.kiru",
            &format!(
                "\
         fn check_fsvs {{\n\
             var shell cwd = `pwd`;\n\
             log $self::cwd;\n\
         }}\n\
         pr test [\n\
             url = `http://example.com`\n\
             dir = `{}`\n\
         ] {{\n\
             use check_fsvs as check;\n\
         }}\n\
         ",
                subdir.to_string_lossy()
            ),
        );
        let cfg = compile_full(&dir.path().join("main.kiru")).unwrap();
        let proj = &cfg.projects["test"];
        let fn_body = &proj.functions["check"];
        assert_eq!(fn_body.len(), 1);
        let stmt = match &fn_body[0] {
            PlanStmt::Log(s) => s,
            other => panic!("expected Log statement, got {:?}", other),
        };
        let expected = std::fs::canonicalize(&subdir)
            .unwrap()
            .to_string_lossy()
            .to_string();
        assert_eq!(**stmt, expected);
    }

    #[test]
    fn test_global_var_shell_uses_current_dir() {
        let dir = tempfile::TempDir::new().unwrap();
        write_config(
            dir.path(),
            "main.kiru",
            "\
         var shell msg = `echo hello-from-global`;\n\
         fn check_gvs { log $global::msg; }\n\
         pr test [\n\
             url = $global::msg\n\
             dir = `d`\n\
         ] {\n\
             use check_gvs as check;\n\
         }\
         ",
        );
        let cfg = compile_full(&dir.path().join("main.kiru")).unwrap();
        let proj = &cfg.projects["test"];
        assert_eq!(proj.url, "hello-from-global");
        let fn_body = &proj.functions["check"];
        let stmt = match &fn_body[0] {
            PlanStmt::Log(s) => s,
            other => panic!("expected Log statement, got {:?}", other),
        };
        assert_eq!(*stmt, "hello-from-global");
    }

    #[test]
    fn test_field_can_reference_project_body_var() {
        let dir = tempfile::TempDir::new().unwrap();
        write_config(
            dir.path(),
            "main.kiru",
            "\
         fn check_frpbv { log $self::x; }\n\
         pr test [\n\
             url = $test::x\n\
             dir = $test::x\n\
         ] {\n\
             var shell x = `echo workspace`;\n\
             use check_frpbv as check;\n\
         }\
         ",
        );
        let cfg = compile_full(&dir.path().join("main.kiru")).unwrap();
        assert_eq!(cfg.projects["test"].url, "workspace");
    }

    #[test]
    fn test_fn_body_var_is_project_global_across_functions() {
        let dir = tempfile::TempDir::new().unwrap();
        write_config(
            dir.path(),
            "main.kiru",
            "\
         fn first_fbpg { log $self::shared; }
         fn second_fbpg { log $self::shared; }
         pr test [url = `u` dir = `d`] {
             var string shared = `VALUE`;
             use first_fbpg as first;
             use second_fbpg as second;
         }
         ",
        );
        let cfg = compile_full(&dir.path().join("main.kiru")).unwrap();
        let fn_body = &cfg.projects["test"].functions["second"];
        let stmt = match &fn_body[0] {
            PlanStmt::Log(s) => s,
            other => panic!("expected Log statement, got {:?}", other),
        };
        assert_eq!(*stmt, "VALUE");
    }

    #[test]
    fn test_field_cannot_reference_fn_body_var() {
        let dir = tempfile::TempDir::new().unwrap();
        write_config(
            dir.path(),
            "main.kiru",
            "\
         fn check_fcrf { var shell x = `echo workspace`; }\n\
         pr test [\n\
             url = $test::x\n\
             dir = $test::x\n\
         ] {\n\
             use check_fcrf as check;\n\
         }\
         ",
        );
        let err = compile_full(&dir.path().join("main.kiru")).unwrap_err();
        assert!(
            err.to_string()
                .contains("cannot reference function-body variable"),
            "got: {}",
            err
        );
    }

    #[test]
    fn test_fn_body_redeclaration_reports_span_without_out_of_bounds() {
        let dir = tempfile::TempDir::new().unwrap();
        write_config(
            dir.path(),
            "main.kiru",
            "\
             fn check_fbrr {
                 var string docker_bin = `y`;
             }
             pr test [ url = `u` dir = `d` ] {
                 var string docker_bin = `x`;
                 use check_fbrr as check;
             }
             ",
        );
        let err = compile_full(&dir.path().join("main.kiru")).unwrap_err();
        let report: &Report = match &err {
            CompileError::ValidationReport(reports) => &reports[0],
            other => panic!("expected ValidationReport, got {}", other),
        };
        let _ = miette::set_hook(Box::new(|_| {
            Box::new(miette::MietteHandlerOpts::new().build())
        }));
        // Render the diagnostic and assert on the sub-phrases that survive
        // miette's Debug trimming (the full help text is not available via
        // `format!("{:?}", ..)`).
        let rendered = format!("{:?}", report);
        assert!(
            rendered.contains("collides with a variable already")
                && rendered.contains("declared in project")
                && rendered.contains("docker_bin"),
            "got: {}",
            rendered
        );
        // The collision must point at the applying project's `use`, not at the
        // reusable function definition.
        assert!(
            rendered.contains("use check_fbrr as check"),
            "diagnostic should point at the `use` site, got: {}",
            rendered
        );
        assert!(
            !rendered.contains("OutOfBounds"),
            "diagnostic leaked an out-of-bounds artifact: {}",
            rendered
        );
        assert!(
            !rendered.contains("<none>"),
            "diagnostic used a default <none> source name: {}",
            rendered
        );
    }

    #[test]
    fn test_project_var_shell_runs_once() {
        let dir = tempfile::TempDir::new().unwrap();
        let subdir = dir.path().join("myproject");
        std::fs::create_dir(&subdir).unwrap();
        let marker = subdir.join("run_count.txt");

        write_config(
            dir.path(),
            "main.kiru",
            &format!(
                "\
         fn check_pvsro {{\n\
             log $self::x;\n\
         }}\n\
         pr test [\n\
             url = `http://example.com`\n\
             dir = `{}`\n\
         ] {{\n\
             var shell x = `echo 1 >> {} && echo done`;\n\
             use check_pvsro as check;\n\
         }}\n\
         ",
                subdir.to_string_lossy(),
                marker.to_string_lossy(),
            ),
        );
        let cfg = compile_full(&dir.path().join("main.kiru")).unwrap();
        let proj = &cfg.projects["test"];
        let fn_body = &proj.functions["check"];
        assert_eq!(fn_body.len(), 1);
        let stmt = match &fn_body[0] {
            PlanStmt::Log(s) => s,
            other => panic!("expected Log statement, got {:?}", other),
        };
        assert_eq!(*stmt, "done");
        let count = std::fs::read_to_string(&marker).unwrap();
        assert_eq!(
            count.lines().count(),
            1,
            "var shell should execute exactly once, got {} lines",
            count.lines().count()
        );
    }

    #[test]
    fn test_cross_project_var_read_is_rejected() {
        // Projects are isolated: reading another project's variable (here a
        // donor body var) is a hard error, not a resolvable reference.
        let dir = tempfile::TempDir::new().unwrap();
        write_config(
            dir.path(),
            "main.kiru",
            "\
             pr a [url = `ua` dir = `da`] {\n\
                 var string shared = `VALUE`;\n\
             }\n\
             pr b [url = $a::shared dir = `db`] { }\
             ",
        );
        let err = compile_full(&dir.path().join("main.kiru")).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("invalid variable reference `a::`")
                && msg.contains("may only reference `self::` or `global::`"),
            "expected isolation error, got: {}",
            msg
        );
    }

    #[test]
    fn test_cross_project_field_read_is_rejected() {
        // A field reading another project (even its metadata) is rejected up
        // front by the isolation check, before any undefined-var reasoning.
        let dir = tempfile::TempDir::new().unwrap();
        write_config(
            dir.path(),
            "main.kiru",
            "\
             var string base = `http://base`;\n\
             pr a [url = $global::base dir = `da`] { }\n\
             pr b [url = $a::url dir = `db`] { }\
             ",
        );
        let err = compile_full(&dir.path().join("main.kiru")).unwrap_err();
        let msg = err.to_string();
        assert!(
            msg.contains("invalid variable reference `a::`"),
            "expected isolation error, got: {}",
            msg
        );
    }

    #[test]
    fn test_self_reference_resolves_to_enclosing_project() {
        // `self::` inside a project resolves to that project's own namespace,
        // so a field and a body var may read the project's own variables.
        let dir = tempfile::TempDir::new().unwrap();
        write_config(
            dir.path(),
            "main.kiru",
            "\
             fn check_srr { log $self::echoed; }\n\
             pr test [\n\
                 url = `http://example.com/${self::name}`\n\
                 dir = `d`\n\
             ] {\n\
                 var string name = `myproject`;\n\
                 var string echoed = $self::name;\n\
                 use check_srr as check;\n\
             }\
             ",
        );
        let cfg = compile_full(&dir.path().join("main.kiru")).unwrap();
        assert_eq!(cfg.projects["test"].url, "http://example.com/myproject");
        let stmt = match &cfg.projects["test"].functions["check"][0] {
            PlanStmt::Log(s) => s,
            other => panic!("expected Log statement, got {:?}", other),
        };
        assert_eq!(*stmt, "myproject");
    }

    #[test]
    fn test_self_reference_at_global_scope() {
        // `self::` at the top level means `global::`.
        let dir = tempfile::TempDir::new().unwrap();
        write_config(
            dir.path(),
            "main.kiru",
            "\
             var string base = `hello`;\n\
             var string derived = `${self::base}-world`;\n\
             pr p [url = $global::derived dir = `d`] { }\
             ",
        );
        let cfg = compile_full(&dir.path().join("main.kiru")).unwrap();
        assert_eq!(cfg.projects["p"].url, "hello-world");
    }

    #[test]
    fn test_exact_duplicate_global_redeclaration_errors() {
        let dir = tempfile::TempDir::new().unwrap();
        write_config(
            dir.path(),
            "main.kiru",
            "\
             var string x = `a`;\n\
             var string x = `b`;\n\
             ",
        );
        let err = compile_full(&dir.path().join("main.kiru")).unwrap_err();
        assert!(err.to_string().contains("already defined"), "got: {}", err);
    }

    #[test]
    fn test_case_arm_var_collision_errors() {
        let dir = tempfile::TempDir::new().unwrap();
        write_config(
            dir.path(),
            "main.kiru",
            "\
             var string os = `x`;\n\
             fn bad_cavc {\n\
                 case $global::os {\n\
                     `Linux` { var string x = `a`; };\n\
                     _ { var string x = `b`; };\n\
                 };\n\
             }\n\
              pr test [\n\
                  url = `u`\n\
                  dir = `d`\n\
              ] {\n\
                  use bad_cavc as bad;\n\
              }\
              ",
        );
        let err = compile_full(&dir.path().join("main.kiru")).unwrap_err();
        assert!(
            err.to_string()
                .contains("collides with a variable already declared in project `test`"),
            "got: {}",
            err
        );
    }
}
