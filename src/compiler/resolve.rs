use crate::compiler::error::{CompileError, spanned_err_named, spanned_err_on_field};
use crate::compiler::fnstmt::{ResolveFnCtx, resolve_fn_body_stmts};
use crate::compiler::scope::{Redeclaration, ScopeKind, ScopeStack};
use crate::compiler::types::{
    Config, Project, ProjectVarStmt, ResolvedCasePattern, SyncMode, UnresolvedConfig,
    UnresolvedProject, parse_sync_mode,
};
use crate::dsl::{CasePattern, Expr, InterpolationPart, Stmt};
use crate::error::SourceFile;
use crate::shell;
use miette::miette;
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

/// Builds the "undefined variable" error for a `$name` reference absent
/// from `scope`. Centralizes the repeated `format!("undefined variable:
/// ${}", ..)` construction used by both expression and case-pattern
/// resolution (bare `VarRef` and interpolated backtick literals).
fn undefined_var_err(
    name: &str,
    offset: usize,
    len: usize,
    sources: &HashMap<String, String>,
    source_name: &str,
) -> CompileError {
    spanned_err_named(
        format!("undefined variable: ${}", name),
        sources,
        source_name,
        offset,
        len,
    )
}

/// Resolves the interpolation `parts` of a backtick literal (or case-
/// pattern literal) into a concrete string, substituting `$name` /
/// `${name}` references against `scope`. Shared by `resolve_expr` and
/// `resolve_case_pattern` so the substitution loop is defined once.
/// On an undefined reference, the error spans the whole literal
/// (`literal_offset`/`literal_len`), matching prior behavior.
fn resolve_interpolation_to_string(
    parts: &[InterpolationPart],
    scope: &ScopeStack<String>,
    literal_offset: usize,
    literal_len: usize,
    sources: &HashMap<String, String>,
    source_name: &str,
) -> Result<String, CompileError> {
    let mut result = String::new();
    for part in parts {
        if part.is_var {
            match scope.lookup(&part.value) {
                Some(val) => result.push_str(val),
                None => {
                    return Err(undefined_var_err(
                        &part.value,
                        literal_offset,
                        literal_len,
                        sources,
                        source_name,
                    ));
                }
            }
        } else {
            result.push_str(&part.value);
        }
    }
    Ok(result)
}

/// Resolve an `Expr` to a concrete string using a scope stack.
pub(crate) fn resolve_expr(
    expr: &Expr,
    scope: &ScopeStack<String>,
    sources: &HashMap<String, String>,
) -> Result<String, CompileError> {
    match expr {
        Expr::VarRef {
            name,
            offset,
            len,
            source_name,
        } => match scope.lookup(name) {
            Some(val) => Ok(val.clone()),
            None => Err(undefined_var_err(name, *offset, *len, sources, source_name)),
        },
        Expr::BacktickLit {
            parts,
            offset,
            len,
            source_name,
        } => resolve_interpolation_to_string(parts, scope, *offset, *len, sources, source_name),
    }
}

/// Resolve a case pattern against a scope stack.
pub(crate) fn resolve_case_pattern(
    pattern: &CasePattern,
    scope: &ScopeStack<String>,
    sources: &HashMap<String, String>,
) -> Result<ResolvedCasePattern, CompileError> {
    match pattern {
        CasePattern::Literal {
            parts,
            offset,
            len,
            source_name,
        } => {
            let resolved =
                resolve_interpolation_to_string(parts, scope, *offset, *len, sources, source_name)?;
            Ok(ResolvedCasePattern::Literal(resolved))
        }
        CasePattern::VarRef {
            name,
            offset,
            len,
            source_name,
        } => match scope.lookup(name) {
            Some(val) => Ok(ResolvedCasePattern::Literal(val.clone())),
            None => Err(undefined_var_err(name, *offset, *len, sources, source_name)),
        },
        CasePattern::Default => Ok(ResolvedCasePattern::Default),
    }
}

/// Resolve and bind a `var` or `var shell` into a scope stack.
/// All duplicate detection flows through `ScopeStack::declare`.
///
/// `working_dir` — the directory in which to execute `var shell` commands;
/// `None` means the current process directory.
/// Resolve a `var` / `var shell` declaration from individual fields (shared
/// implementation for both `resolve_var_stmt` and `resolve_project_var`).
#[allow(clippy::too_many_arguments)]
fn resolve_var_stmt_inner(
    var_type: &crate::dsl::VarType,
    name: &str,
    value: &Expr,
    offset: usize,
    len: usize,
    scope: &mut ScopeStack<String>,
    working_dir: Option<&Path>,
    sources: &HashMap<String, String>,
) -> Result<(), CompileError> {
    let source = SourceFile::from_registry(sources, value.source_name());
    if let Some(existing_kind) = scope.declaring_kind(name) {
        return Err(redeclaration_err(
            Redeclaration {
                name: name.to_owned(),
                existing_kind,
            },
            sources,
            value.source_name(),
            offset,
            len,
        ));
    }
    let resolved = resolve_expr(value, scope, sources)?;
    let final_val = if *var_type == crate::dsl::VarType::Shell {
        shell::execute_shell_variable(name, &resolved, working_dir, &source, offset, len)?
    } else {
        resolved
    };
    scope
        .declare(name.to_owned(), final_val)
        .map_err(|r| redeclaration_err(r, sources, value.source_name(), offset, len))?;
    Ok(())
}

/// Resolve a `var` / `var shell` from a full `Stmt::Var` AST node (linear
/// phase for top-level vars outside project blocks).
pub(crate) fn resolve_var_stmt(
    stmt: &Stmt,
    scope: &mut ScopeStack<String>,
    working_dir: Option<&Path>,
    sources: &HashMap<String, String>,
) -> Result<(), CompileError> {
    if let Stmt::Var {
        var_type,
        name,
        value,
        offset,
        len,
        ..
    } = stmt
    {
        resolve_var_stmt_inner(
            var_type,
            name,
            value,
            *offset,
            *len,
            scope,
            working_dir,
            sources,
        )
    } else {
        Ok(())
    }
}

/// Resolve a `var` / `var shell` from a `ProjectVarStmt` (second pass in
/// `resolve_with_scopes`).
pub(crate) fn resolve_project_var(
    var: &ProjectVarStmt,
    scope: &mut ScopeStack<String>,
    working_dir: Option<&Path>,
    sources: &HashMap<String, String>,
) -> Result<(), CompileError> {
    resolve_var_stmt_inner(
        &var.var_type,
        &var.name,
        &var.value,
        var.offset,
        var.len,
        scope,
        working_dir,
        sources,
    )
}

/// Build a spanned error from a `Redeclaration`, located on the node that
/// re-declares the name (resolved against the source-text registry by name).
pub(crate) fn redeclaration_err(
    r: Redeclaration,
    sources: &HashMap<String, String>,
    name: &str,
    offset: usize,
    len: usize,
) -> CompileError {
    let msg = format!("${} is already defined at {}", r.name, r.existing_kind);
    spanned_err_named(msg, sources, name, offset, len)
}

/// Resolve an optional `Expr` field to a concrete string.
pub(crate) fn resolve_optional_expr(
    expr: &Option<Expr>,
    scope: &ScopeStack<String>,
    sources: &HashMap<String, String>,
) -> Result<Option<String>, CompileError> {
    match expr {
        Some(e) => {
            let resolved = resolve_expr(e, scope, sources)?;
            if resolved.is_empty() {
                Ok(None)
            } else {
                Ok(Some(resolved))
            }
        }
        None => Ok(None),
    }
}

/// Resolve a `dir` field, joining relative paths against the source file's
/// directory so that `dir = \`./foo\`` resolves relative to the `.kiru` file.
fn resolve_dir_field(
    unresolved: &UnresolvedProject,
    scope: &ScopeStack<String>,
    sources: &HashMap<String, String>,
) -> Result<String, CompileError> {
    let raw = resolve_optional_expr(&unresolved.dir, scope, sources)?.unwrap_or_default();
    if raw.is_empty() || Path::new(&raw).is_absolute() {
        return Ok(raw);
    }
    let dir_source_name = unresolved
        .dir
        .as_ref()
        .map(|e| e.source_name())
        .unwrap_or(unresolved.source_file.as_str());
    let base_dir = Path::new(dir_source_name).parent().ok_or_else(|| {
        spanned_err_on_field(
            "cannot determine base directory for dir".to_string(),
            sources,
            &unresolved.dir,
            &unresolved.source_file,
        )
    })?;
    Ok(base_dir.join(&raw).to_string_lossy().to_string())
}

/// Resolve an unresolved project's field expressions against a combined
/// scope that includes both global and project-level vars. Returns the
/// four resolved field values as a tuple `(url, dir, sync, branch)`.
pub(crate) fn resolve_project_fields(
    unresolved: &UnresolvedProject,
    scope: &ScopeStack<String>,
    sources: &HashMap<String, String>,
) -> Result<(String, String, SyncMode, Option<String>), CompileError> {
    let url = resolve_optional_expr(&unresolved.url, scope, sources)?.unwrap_or_default();
    let dir = resolve_dir_field(unresolved, scope, sources)?;
    let sync = match resolve_optional_expr(&unresolved.sync, scope, sources)? {
        Some(mode) => parse_sync_mode(&mode).map_err(|msg| {
            spanned_err_on_field(msg, sources, &unresolved.sync, &unresolved.source_file)
        })?,
        None => SyncMode::Clone,
    };
    let branch = resolve_optional_expr(&unresolved.branch, scope, sources)?;
    Ok((url, dir, sync, branch))
}

/// Resolve using pre-computed scopes.
///
/// `force_cwd` mirrors the `KIRU_CWD` env var: when set, project-body
/// `var shell` commands run in the current directory instead of the
/// resolved project directory.
pub(crate) fn resolve_with_scopes(
    unresolved: UnresolvedConfig,
    global: ScopeStack<String>,
    sources: &HashMap<String, String>,
    force_cwd: bool,
) -> Result<Config, CompileError> {
    let mut projects = HashMap::new();
    let mut seen_dirs: HashSet<String> = HashSet::new();
    for (name, unresolved_project) in unresolved.projects {
        // 1. Project fields are resolved against the GLOBAL scope only.
        //    They may reference global vars (and earlier fields), never the
        //    project's own body vars — those are encapsulated by the project
        //    and resolved below. This also means a `var shell` interpolation
        //    inside a field always runs in the current directory, since the
        //    project directory is not yet known.
        let (url, dir, sync, branch) =
            resolve_project_fields(&unresolved_project, &global, sources)?;

        if !dir.is_empty() && !seen_dirs.insert(dir.clone()) {
            return Err(CompileError::ValidationReport(vec![miette!(
                "project {:?}: duplicate directory {:?}",
                name,
                dir
            )]));
        }

        // 2. The project body runs in the resolved project directory (or the
        //    current directory when force_cwd is set / dir is empty).
        let effective_dir: Option<PathBuf> = if force_cwd || dir.is_empty() {
            None
        } else {
            Some(PathBuf::from(&dir))
        };
        let working_dir: Option<&Path> = effective_dir.as_deref();

        // 3. One project frame for the whole body — no re-push, no two-phase
        //    re-resolution. Body vars and function bodies all resolve once,
        //    against this single frame, in the project directory.
        let mut project_scope = global.clone();
        project_scope.push_frame(ScopeKind::Project);

        // 4. Resolve body var statements once, in the project directory.
        for var_stmt in &unresolved_project.var_stmts {
            resolve_project_var(var_stmt, &mut project_scope, working_dir, sources)?;
        }

        // 5. Resolve each function body against the project frame.
        let mut functions = HashMap::new();
        for (fn_name, body) in &unresolved_project.functions {
            // Push a Function frame via RAII guard — no clone needed.
            let guard = project_scope.enter(ScopeKind::Function);
            let mut resolve_ctx = ResolveFnCtx {
                scope: &mut *guard.stack,
                working_dir,
                sources,
            };
            let resolved_body = resolve_fn_body_stmts(body, &mut resolve_ctx)?;
            functions.insert(fn_name.clone(), resolved_body);
            // guard drops here, popping the Function frame
        }

        projects.insert(
            name,
            Project {
                url,
                dir,
                sync,
                branch,
                functions,
                runs: unresolved_project.runs,
            },
        );
    }

    Ok(Config { projects })
}

#[cfg(test)]
mod tests {
    use crate::compiler::error::CompileError;
    use crate::compiler::fnstmt::ResolvedFnStmt;
    use crate::compiler::test_support::*;
    use miette::Report;

    #[test]
    fn test_variable_chain_resolution() {
        let dir = tempfile::TempDir::new().unwrap();
        write_config(
            dir.path(),
            "main.kiru",
            "\
        var string a = `x`;\n\
        var string b = $a;\n\
        var string c = $b;\n\
        pr p [url = $c dir = `d`] { }
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
        pr p [url = `http://${name}.com` dir = `d`] { }\
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
        // The `dir` value is defined in sub/build.kiru, so it must resolve
        // relative to that file's directory (sub/), not the first-merged
        // declaration's file (main.kiru at the project root).
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
        pr x [url = $myurl dir = `d`] { }\
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
            var string b = $a;\n\
        }\
        ",
        );
        // We can't check project vars directly on the resolved Config,
        // but the configuration should compile and resolve without errors.
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
    fn test_project_field_interpolation_cannot_reference_body_var() {
        let dir = tempfile::TempDir::new().unwrap();
        write_config(
            dir.path(),
            "main.kiru",
            "\
        pr test [\n\
            url = `http://example.com/${name}`\n\
            dir = $name\n\
        ] {\n\
            var string name = `myproject`;\n\
        }\
        ",
        );
        let result = compile_full(&dir.path().join("main.kiru"));
        assert!(result.is_err());
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
        pr test [\n\
            url = `http://example.com`\n\
            dir = `{}`\n\
        ] {{\n\
            var shell cwd = `pwd`;\n\
            fn check {{\n\
                log $cwd;\n\
            }}\n\
        }}\n\
        ",
                subdir.to_string_lossy()
            ),
        );

        let cfg = compile_full_with_cwd(&dir.path().join("main.kiru"), true).unwrap();

        let proj = &cfg.projects["test"];
        let fn_body = &proj.functions["check"];
        assert_eq!(fn_body.len(), 1);
        let stmt = match &fn_body[0] {
            ResolvedFnStmt::Log(s) => s,
            other => panic!("expected Log statement, got {:?}", other),
        };
        let expected = current_dir.to_string_lossy().to_string();
        assert_eq!(*stmt.value, expected);
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
        pr test [\n\
            url = `http://example.com`\n\
            dir = `{}`\n\
        ] {{\n\
            var shell cwd = `pwd`;\n\
            fn check {{\n\
                log $cwd;\n\
            }}\n\
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
            ResolvedFnStmt::Log(s) => s,
            other => panic!("expected Log statement, got {:?}", other),
        };
        let expected = std::fs::canonicalize(&subdir)
            .unwrap()
            .to_string_lossy()
            .to_string();
        assert_eq!(*stmt.value, expected);
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
        pr test [\n\
            url = `http://example.com`\n\
            dir = `{}`\n\
        ] {{\n\
            fn check {{\n\
                var shell cwd = `pwd`;\n\
                log $cwd;\n\
            }}\n\
        }}\n\
        ",
                subdir.to_string_lossy()
            ),
        );
        let cfg = compile_full(&dir.path().join("main.kiru")).unwrap();
        let proj = &cfg.projects["test"];
        let fn_body = &proj.functions["check"];
        assert_eq!(fn_body.len(), 1); // VarDecl consumed, only log emitted
        let stmt = match &fn_body[0] {
            ResolvedFnStmt::Log(s) => s,
            other => panic!("expected Log statement, got {:?}", other),
        };
        let expected = std::fs::canonicalize(&subdir)
            .unwrap()
            .to_string_lossy()
            .to_string();
        assert_eq!(*stmt.value, expected);
    }

    #[test]
    fn test_global_var_shell_uses_current_dir() {
        let dir = tempfile::TempDir::new().unwrap();
        write_config(
            dir.path(),
            "main.kiru",
            "\
        var shell msg = `echo hello-from-global`;\n\
        pr test [\n\
            url = $msg\n\
            dir = `d`\n\
        ] {\n\
            fn check { log $msg; }\n\
        }\
        ",
        );
        let cfg = compile_full(&dir.path().join("main.kiru")).unwrap();
        let proj = &cfg.projects["test"];
        assert_eq!(proj.url, "hello-from-global");
        let fn_body = &proj.functions["check"];
        let stmt = match &fn_body[0] {
            ResolvedFnStmt::Log(s) => s,
            other => panic!("expected Log statement, got {:?}", other),
        };
        assert_eq!(stmt.value, "hello-from-global");
    }

    #[test]
    fn test_field_cannot_reference_project_body_var() {
        let dir = tempfile::TempDir::new().unwrap();
        write_config(
            dir.path(),
            "main.kiru",
            "\
        pr test [\n\
            url = $x\n\
            dir = $x\n\
        ] {\n\
            var shell x = `echo workspace`;\n\
            fn check { log $x; }\n\
        }\
        ",
        );
        // Fields are resolved against the global scope only and may not reach
        // into the project body, so a field referencing a body var is an
        // undefined-variable error.  There is no cycle to deadlock on because
        // the project directory is computed before the body is ever resolved.
        let err = compile_full(&dir.path().join("main.kiru")).unwrap_err();
        assert!(
            err.to_string().contains("undefined variable"),
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
            pr test [ url = `u` dir = `d` ] {
                var string docker_bin = `x`;
                fn check {
                    var string docker_bin = `y`;
                }
            }
            ",
        );
        let err = compile_full(&dir.path().join("main.kiru")).unwrap_err();
        let report: &Report = match &err {
            CompileError::ValidationReport(reports) => &reports[0],
            other => panic!("expected ValidationReport, got {}", other),
        };
        // Render through the graphical handler — this is exactly where the
        // `[Failed to read contents for label <none> ... OutOfBounds]` artifact
        // used to leak when function-body spans pointed at an empty source.
        let _ = miette::set_hook(Box::new(|_| {
            Box::new(miette::MietteHandlerOpts::new().build())
        }));
        let rendered = format!("{:?}", report);
        assert!(
            rendered.contains("already defined at project"),
            "got: {}",
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
        pr test [\n\
            url = `http://example.com`\n\
            dir = `{}`\n\
        ] {{\n\
            var shell x = `echo 1 >> {} && echo done`;\n\
            fn check {{\n\
                log $x;\n\
            }}\n\
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
            ResolvedFnStmt::Log(s) => s,
            other => panic!("expected Log statement, got {:?}", other),
        };
        assert_eq!(stmt.value, "done");
        let count = std::fs::read_to_string(&marker).unwrap();
        assert_eq!(
            count.lines().count(),
            1,
            "var shell should execute exactly once, got {} lines",
            count.lines().count()
        );
    }
}
