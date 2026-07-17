use crate::compiler::error::{CompileError, SourceFile, spanned_err_named, spanned_err_on_field};
use crate::compiler::scope::{Redeclaration, ScopeKind, ScopeStack};
use crate::compiler::types::{
    Config, Project, ProjectVarStmt, ResolvedCaseArm, ResolvedCasePattern, ResolvedEnvPair,
    ResolvedFnStmt, SyncMode, UnresolvedConfig, UnresolvedProject,
};
use crate::dsl::{CaseArm, CasePattern, EnvPair, Expr, FnStmt, InterpolationPart, Stmt};
use crate::shell;
use miette::miette;
use std::cell::Cell;
use std::collections::{HashMap, HashSet};
use std::path::Path;

// Thread-local override for `KIRU_CWD` env var, used by tests so they
// don't race writing to process-global `std::env::set_var`.
thread_local! {
    static KIRU_CWD_OVERRIDE: Cell<Option<bool>> = const { Cell::new(None) };
}

#[doc(hidden)]
pub fn __test_set_kiru_cwd(val: Option<bool>) -> Option<bool> {
    KIRU_CWD_OVERRIDE.replace(val)
}

/// Invoke a callback with the name of every variable referenced in `expr`,
/// whether as a bare `$name` or an interpolation `${name}` in a backtick literal.
pub(crate) fn visit_expr_vars(expr: &Expr, mut f: impl FnMut(&str)) {
    match expr {
        Expr::VarRef { name, .. } => f(name),
        Expr::BacktickLit { parts, .. } => {
            for part in parts {
                if part.is_var {
                    f(&part.value);
                }
            }
        }
    }
}

/// Invoke a callback with the name of every variable referenced in a case
/// pattern, including bare `$name`, backtick interpolation `${name}`, and
/// default (`_`) patterns (which reference no variables).
pub(crate) fn visit_case_pattern_vars(pattern: &CasePattern, mut f: impl FnMut(&str)) {
    match pattern {
        CasePattern::VarRef { name, .. } => f(name),
        CasePattern::Literal { parts, .. } => {
            for part in parts {
                if part.is_var {
                    f(&part.value);
                }
            }
        }
        CasePattern::Default => {}
    }
}

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
fn resolve_case_pattern(
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

/// Parse the sync mode string from a resolved value.
pub(crate) fn parse_sync_mode_value(value: &str) -> Result<SyncMode, String> {
    match value {
        "clone" => Ok(SyncMode::Clone),
        "ignore" => Ok(SyncMode::Ignore),
        _ => Err(format!(
            "invalid sync value {:?} (expected 'clone' or 'ignore')",
            value
        )),
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

/// Resolve an unresolved project's field expressions against a combined scope
/// that includes both global and project-level vars.
pub(crate) fn resolve_project_fields(
    unresolved: &UnresolvedProject,
    scope: &ScopeStack<String>,
    sources: &HashMap<String, String>,
) -> Result<(String, String, SyncMode, Option<String>), CompileError> {
    let url = resolve_optional_expr(&unresolved.url, scope, sources)?.unwrap_or_default();
    let dir = resolve_dir_field(unresolved, scope, sources)?;
    let sync = match resolve_optional_expr(&unresolved.sync, scope, sources)? {
        Some(mode) => parse_sync_mode_value(&mode).map_err(|msg| {
            spanned_err_on_field(msg, sources, &unresolved.sync, &unresolved.source_file)
        })?,
        None => SyncMode::Clone,
    };
    let branch = resolve_optional_expr(&unresolved.branch, scope, sources)?;
    Ok((url, dir, sync, branch))
}

/// Resolve using pre-computed scopes.
pub(crate) fn resolve_with_scopes(
    unresolved: UnresolvedConfig,
    global: ScopeStack<String>,
    sources: &HashMap<String, String>,
) -> Result<Config, CompileError> {
    let mut projects = HashMap::new();
    let mut seen_dirs: HashSet<String> = HashSet::new();
    for (name, unresolved_project) in unresolved.projects {
        // Build a combined scope (global + project vars) once per project
        // and reuse it for both field resolution and function-body resolution.
        let mut project_scope = global.clone();
        project_scope.push_frame(ScopeKind::Project);
        project_scope.seed_top(unresolved_project.vars.clone());

        let (url, dir, sync, branch) =
            resolve_project_fields(&unresolved_project, &project_scope, sources)?;

        if !dir.is_empty() && !seen_dirs.insert(dir.clone()) {
            return Err(CompileError::ValidationReport(vec![miette!(
                "project {:?}: duplicate directory {:?}",
                name,
                dir
            )]));
        }

        // ── Re-resolve project-scope var stmts with project dir ────────
        // After `resolve_project_fields` we know the final `dir`.  Pop the
        // Project frame that was seeded with linear-phase values, re-push
        // one, and resolve the raw var stmts in order with the correct
        // working directory so `var shell` commands inside project body
        // execute in the project dir.  Fields that interpolated a project
        // shell var already used the linear-phase (current-dir) value —
        // that is correct per the ordering rule (item 4 in todo.md).
        let use_cwd = KIRU_CWD_OVERRIDE
            .with(|cell| cell.get())
            .unwrap_or_else(|| std::env::var("KIRU_CWD").as_deref() == Ok("1"));
        let working_dir: Option<&Path> = if use_cwd || dir.is_empty() {
            None
        } else {
            Some(Path::new(&dir))
        };

        // Rebuild the project frame with fresh values.
        let (_prev_entries, _prev_reserved) = project_scope.pop_frame_entries();
        project_scope.push_frame(ScopeKind::Project);
        for var_stmt in &unresolved_project.var_stmts {
            if unresolved_project.field_refd_vars.contains(&var_stmt.name) {
                // Seed the pre-resolved (linear-phase) value — these
                // ran with current-dir so fields that reference them
                // are consistent.  Do NOT re-execute.
                let val = unresolved_project
                    .vars
                    .get(&var_stmt.name)
                    .cloned()
                    .unwrap_or_default();
                project_scope
                    .declare(var_stmt.name.clone(), val)
                    .map_err(|r| {
                        redeclaration_err(
                            r,
                            sources,
                            &var_stmt.source_name,
                            var_stmt.offset,
                            var_stmt.len,
                        )
                    })?;
            } else {
                resolve_project_var(var_stmt, &mut project_scope, working_dir, sources)?;
            }
        }

        let mut functions = HashMap::new();

        for (fn_name, body) in &unresolved_project.functions {
            // Push a Function frame via RAII guard — no clone needed.
            let guard = project_scope.enter(ScopeKind::Function);
            let resolved_body =
                resolve_fn_body_inner(body, &mut *guard.stack, working_dir, sources)?;
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

/// Resolve an env block: resolve each pair's value, then resolve the body
/// in the same scope (no isolated var scope for env blocks).
fn resolve_env_block_stmt(
    pairs: &[EnvPair],
    body: &[FnStmt],
    scope: &mut ScopeStack<String>,
    working_dir: Option<&Path>,
    sources: &HashMap<String, String>,
) -> Result<ResolvedFnStmt, CompileError> {
    let mut resolved_pairs = Vec::new();
    for pair in pairs {
        let resolved_value = resolve_expr(&pair.value, scope, sources)?;
        resolved_pairs.push(ResolvedEnvPair {
            key: pair.key.clone(),
            value: resolved_value,
        });
    }
    let resolved_body = resolve_fn_body_inner(body, scope, working_dir, sources)?;
    Ok(ResolvedFnStmt::EnvBlock {
        pairs: resolved_pairs,
        body: resolved_body,
    })
}

/// Resolve a case statement: resolve the condition, then resolve each
/// arm's pattern and body with a new scope frame per arm.
fn resolve_case_stmt(
    condition: &Expr,
    scopes: &[CaseArm],
    scope: &mut ScopeStack<String>,
    working_dir: Option<&Path>,
    sources: &HashMap<String, String>,
) -> Result<ResolvedFnStmt, CompileError> {
    let resolved_condition = resolve_expr(condition, scope, sources)?;
    let mut resolved_scopes = Vec::new();
    for arm in scopes {
        let pattern = resolve_case_pattern(&arm.pattern, scope, sources)?;
        let guard = scope.enter(ScopeKind::Case);
        let body = resolve_fn_body_inner(&arm.body, guard.stack, working_dir, sources)?;
        resolved_scopes.push(ResolvedCaseArm { pattern, body });
    }
    Ok(ResolvedFnStmt::Case {
        condition: resolved_condition,
        scopes: resolved_scopes,
    })
}

/// Recursively resolve a function body by dispatching each statement to
/// the appropriate resolver.
fn resolve_fn_body_inner(
    body: &[FnStmt],
    scope: &mut ScopeStack<String>,
    working_dir: Option<&Path>,
    sources: &HashMap<String, String>,
) -> Result<Vec<ResolvedFnStmt>, CompileError> {
    let mut resolved = Vec::new();
    for stmt in body {
        match stmt {
            FnStmt::VarDecl {
                var_type,
                name,
                value,
            } => {
                let (offset, len) = value.offset_len();
                let source = SourceFile::from_registry(sources, value.source_name());
                let resolved_value = resolve_expr(value, scope, sources)?;
                let final_value = if *var_type == crate::dsl::VarType::Shell {
                    shell::execute_shell_variable(
                        name,
                        &resolved_value,
                        working_dir,
                        &source,
                        offset,
                        len,
                    )?
                } else {
                    resolved_value
                };
                scope
                    .declare(name.to_string(), final_value)
                    .map_err(|r| redeclaration_err(r, sources, value.source_name(), offset, len))?;
            }
            FnStmt::Log { value } => {
                let v = resolve_expr(value, scope, sources)?;
                resolved.push(ResolvedFnStmt::Log { value: v });
            }
            FnStmt::Exec { value } => {
                let v = resolve_expr(value, scope, sources)?;
                resolved.push(ResolvedFnStmt::Exec { value: v });
            }
            FnStmt::Cd { value } => {
                let v = resolve_expr(value, scope, sources)?;
                resolved.push(ResolvedFnStmt::Cd { value: v });
            }
            FnStmt::EnvBlock { pairs, body } => {
                resolved.push(resolve_env_block_stmt(
                    pairs,
                    body,
                    scope,
                    working_dir,
                    sources,
                )?);
            }
            FnStmt::Case { condition, scopes } => {
                resolved.push(resolve_case_stmt(
                    condition,
                    scopes,
                    scope,
                    working_dir,
                    sources,
                )?);
            }
        }
    }
    Ok(resolved)
}

#[cfg(test)]
mod tests {
    use crate::compiler::error::CompileError;
    use crate::compiler::test_support::*;
    use crate::compiler::types::ResolvedFnStmt;
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
    fn test_project_field_references_project_var() {
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
        let cfg = compile_full(&dir.path().join("main.kiru")).unwrap();
        let proj = &cfg.projects["test"];
        assert_eq!(proj.url, "http://example.com/myproject");
        assert_eq!(proj.dir, dir.path().join("myproject").to_string_lossy());
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

        let _guard = KiruCwdGuard::with_kiru_cwd();
        let cfg = compile_full(&dir.path().join("main.kiru")).unwrap();

        let proj = &cfg.projects["test"];
        let fn_body = &proj.functions["check"];
        assert_eq!(fn_body.len(), 1);
        match &fn_body[0] {
            ResolvedFnStmt::Log { value } => {
                let expected = current_dir.to_string_lossy().to_string();
                assert_eq!(*value, expected);
            }
            other => panic!("expected Log, got {:?}", other),
        }
    }

    #[test]
    fn test_project_scope_var_shell_uses_project_dir() {
        let _guard = KiruCwdGuard::with_project_dir();

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
        match &fn_body[0] {
            ResolvedFnStmt::Log { value } => {
                let expected = std::fs::canonicalize(&subdir)
                    .unwrap()
                    .to_string_lossy()
                    .to_string();
                assert_eq!(*value, expected);
            }
            other => panic!("expected Log, got {:?}", other),
        }
    }

    #[test]
    fn test_fn_scope_var_shell_uses_project_dir() {
        let _guard = KiruCwdGuard::with_project_dir();
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
        match &fn_body[0] {
            ResolvedFnStmt::Log { value } => {
                let expected = std::fs::canonicalize(&subdir)
                    .unwrap()
                    .to_string_lossy()
                    .to_string();
                assert_eq!(*value, expected);
            }
            other => panic!("expected Log, got {:?}", other),
        }
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
        match &fn_body[0] {
            ResolvedFnStmt::Log { value } => {
                assert_eq!(value, "hello-from-global");
            }
            other => panic!("expected Log, got {:?}", other),
        }
    }

    #[test]
    fn test_var_shell_used_in_dir_field_no_deadlock() {
        let _guard = KiruCwdGuard::with_project_dir();
        let dir = tempfile::TempDir::new().unwrap();
        // The `dir` field resolves to `$x` (linear-phase value "workspace"),
        // which gets joined with the source directory.  Create that directory
        // so the re-resolved shell can spawn there.
        let resolved_dir = dir.path().join("workspace");
        std::fs::create_dir(&resolved_dir).unwrap();

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
        // Must not deadlock/cycle.  The dir field uses the linear-phase value
        // of $x (current-dir shell execution), so dir resolves to a relative
        // path that is joined with the source file's directory.
        let cfg = compile_full(&dir.path().join("main.kiru")).unwrap();
        let proj = &cfg.projects["test"];
        assert!(proj.url.contains("workspace"));
        assert!(proj.dir.contains("workspace"));
        // The re-resolved x (in project dir) is also "workspace" because
        // `echo` doesn't depend on working directory.
        let fn_body = &proj.functions["check"];
        match &fn_body[0] {
            ResolvedFnStmt::Log { value } => {
                assert_eq!(value, "workspace");
            }
            other => panic!("expected Log, got {:?}", other),
        }
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
        let _guard = KiruCwdGuard::with_project_dir();
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
        match &fn_body[0] {
            ResolvedFnStmt::Log { value } => {
                assert_eq!(value, "done");
            }
            other => panic!("expected Log, got {:?}", other),
        }
        let count = std::fs::read_to_string(&marker).unwrap();
        assert_eq!(
            count.lines().count(),
            1,
            "var shell should execute exactly once, got {} lines",
            count.lines().count()
        );
    }
}
