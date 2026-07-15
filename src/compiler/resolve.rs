use crate::compiler::error::CompileError;
use crate::compiler::error::spanned_err;
use crate::compiler::scope::{Redeclaration, ScopeKind, ScopeStack};
use crate::compiler::types::{
    Config, Project, ProjectVarStmt, ResolvedCaseArm, ResolvedCasePattern, ResolvedEnvPair,
    ResolvedFnStmt, SyncMode, UnresolvedConfig, UnresolvedProject,
};
use crate::dsl::{CaseArm, CasePattern, EnvPair, Expr, FnStmt, Stmt};
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

/// Resolve an `Expr` to a concrete string using a scope stack.
pub(crate) fn resolve_expr(
    expr: &Expr,
    scope: &ScopeStack<String>,
    source_name: &str,
    source_text: &str,
) -> Result<String, CompileError> {
    let make_span_error =
        |msg: String, o: usize, l: usize| spanned_err(msg, source_name, source_text, o, l);
    match expr {
        Expr::VarRef { name, offset, len } => match scope.lookup(name) {
            Some(val) => Ok(val.clone()),
            None => Err(make_span_error(
                format!("undefined variable: ${}", name),
                *offset,
                *len,
            )),
        },
        Expr::BacktickLit { parts, offset, len } => {
            let mut result = String::new();
            for part in parts {
                if part.is_var {
                    match scope.lookup(&part.value) {
                        Some(val) => result.push_str(val),
                        None => {
                            return Err(make_span_error(
                                format!("undefined variable: ${}", part.value),
                                *offset,
                                *len,
                            ));
                        }
                    }
                } else {
                    result.push_str(&part.value);
                }
            }
            Ok(result)
        }
    }
}

/// Resolve a case pattern against a scope stack.
fn resolve_case_pattern(
    pattern: &CasePattern,
    scope: &ScopeStack<String>,
    source_name: &str,
    source_text: &str,
) -> Result<ResolvedCasePattern, CompileError> {
    let make_span_error =
        |msg: String, o: usize, l: usize| spanned_err(msg, source_name, source_text, o, l);
    match pattern {
        CasePattern::Literal { parts, offset, len } => {
            let mut result = String::new();
            for part in parts {
                if part.is_var {
                    match scope.lookup(&part.value) {
                        Some(val) => result.push_str(val),
                        None => {
                            return Err(make_span_error(
                                format!("undefined variable: ${}", part.value),
                                *offset,
                                *len,
                            ));
                        }
                    }
                } else {
                    result.push_str(&part.value);
                }
            }
            Ok(ResolvedCasePattern::Literal(result))
        }
        CasePattern::VarRef { name, offset, len } => match scope.lookup(name) {
            Some(val) => Ok(ResolvedCasePattern::Literal(val.clone())),
            None => Err(make_span_error(
                format!("undefined variable: ${}", name),
                *offset,
                *len,
            )),
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
    source_name: &str,
    source_text: &str,
) -> Result<(), CompileError> {
    let resolved = resolve_expr(value, scope, source_name, source_text)?;
    let final_val = if *var_type == crate::dsl::VarType::Shell {
        shell::execute_shell_variable(
            name,
            &resolved,
            working_dir,
            source_name,
            source_text,
            offset,
            len,
        )?
    } else {
        resolved
    };
    scope
        .declare(name.to_owned(), final_val)
        .map_err(|r| redeclaration_err(r, source_name, source_text, offset, len))?;
    Ok(())
}

/// Resolve a `var` / `var shell` from a full `Stmt::Var` AST node (linear
/// phase for top-level vars outside project blocks).
pub(crate) fn resolve_var_stmt(
    stmt: &Stmt,
    scope: &mut ScopeStack<String>,
    working_dir: Option<&Path>,
    source_name: &str,
    source_text: &str,
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
            source_name,
            source_text,
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
    source_name: &str,
    source_text: &str,
) -> Result<(), CompileError> {
    resolve_var_stmt_inner(
        &var.var_type,
        &var.name,
        &var.value,
        var.offset,
        var.len,
        scope,
        working_dir,
        source_name,
        source_text,
    )
}

/// Build a spanned error from a `Redeclaration`.
pub(crate) fn redeclaration_err(
    r: Redeclaration,
    source_name: &str,
    source_text: &str,
    offset: usize,
    len: usize,
) -> CompileError {
    let msg = format!("${} is already defined at {}", r.name, r.existing_kind);
    spanned_err(msg, source_name, source_text, offset, len)
}

/// Resolve an optional `Expr` field to a concrete string.
pub(crate) fn resolve_optional_expr(
    expr: &Option<Expr>,
    scope: &ScopeStack<String>,
    source_name: &str,
    source_text: &str,
) -> Result<Option<String>, CompileError> {
    match expr {
        Some(e) => {
            let resolved = resolve_expr(e, scope, source_name, source_text)?;
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
) -> Result<String, CompileError> {
    let raw = resolve_optional_expr(&unresolved.dir, scope, "", "")?.unwrap_or_default();
    if raw.is_empty() || Path::new(&raw).is_absolute() {
        return Ok(raw);
    }
    let base_dir = Path::new(&unresolved.source_file).parent().ok_or_else(|| {
        spanned_err(
            "cannot determine base directory for dir".to_string(),
            "",
            "",
            0,
            0,
        )
    })?;
    Ok(base_dir.join(&raw).to_string_lossy().to_string())
}

/// Resolve an unresolved project's field expressions against a combined scope
/// that includes both global and project-level vars.
pub(crate) fn resolve_project_fields(
    unresolved: &UnresolvedProject,
    scope: &ScopeStack<String>,
) -> Result<(String, String, SyncMode, Option<String>), CompileError> {
    let sync_offset_len = unresolved
        .sync
        .as_ref()
        .map(|e| e.offset_len())
        .unwrap_or((0, 1));
    let url = resolve_optional_expr(&unresolved.url, scope, "", "")?.unwrap_or_default();
    let dir = resolve_dir_field(unresolved, scope)?;
    let sync = match resolve_optional_expr(&unresolved.sync, scope, "", "")? {
        Some(mode) => {
            let (sync_offset, sync_len) = sync_offset_len;
            parse_sync_mode_value(&mode)
                .map_err(|msg| spanned_err(msg, "", "", sync_offset, sync_len))?
        }
        None => SyncMode::Clone,
    };
    let branch = resolve_optional_expr(&unresolved.branch, scope, "", "")?;
    Ok((url, dir, sync, branch))
}

/// Resolve using pre-computed scopes.
pub(crate) fn resolve_with_scopes(
    unresolved: UnresolvedConfig,
    global: ScopeStack<String>,
) -> Result<Config, CompileError> {
    let mut projects = HashMap::new();
    let mut seen_dirs: HashSet<String> = HashSet::new();
    for (name, unresolved_project) in unresolved.projects {
        // Build a combined scope (global + project vars) once per project
        // and reuse it for both field resolution and function-body resolution.
        let mut project_scope = global.clone();
        project_scope.push_frame(ScopeKind::Project);
        project_scope.seed_top(unresolved_project.vars.clone());

        let (url, dir, sync, branch) = resolve_project_fields(&unresolved_project, &project_scope)?;

        if !dir.is_empty() && !seen_dirs.insert(dir.clone()) {
            return Err(CompileError::ValidationReport(miette!(
                "project {:?}: duplicate directory {:?}",
                name,
                dir
            )));
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
                            &unresolved_project.source_file,
                            &unresolved_project.source_text,
                            var_stmt.offset,
                            var_stmt.len,
                        )
                    })?;
            } else {
                resolve_project_var(
                    var_stmt,
                    &mut project_scope,
                    working_dir,
                    &unresolved_project.source_file,
                    &unresolved_project.source_text,
                )?;
            }
        }

        let mut functions = HashMap::new();

        for (fn_name, body) in &unresolved_project.functions {
            // Push a Function frame via RAII guard — no clone needed.
            let guard = project_scope.enter(ScopeKind::Function);
            let resolved_body =
                resolve_fn_body_inner(body, &mut *guard.stack, working_dir, "", "")?;
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
    source_name: &str,
    source_text: &str,
) -> Result<ResolvedFnStmt, CompileError> {
    let mut resolved_pairs = Vec::new();
    for pair in pairs {
        let resolved_value = resolve_expr(&pair.value, scope, source_name, source_text)?;
        resolved_pairs.push(ResolvedEnvPair {
            key: pair.key.clone(),
            value: resolved_value,
        });
    }
    let resolved_body = resolve_fn_body_inner(body, scope, working_dir, source_name, source_text)?;
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
    source_name: &str,
    source_text: &str,
) -> Result<ResolvedFnStmt, CompileError> {
    let resolved_condition = resolve_expr(condition, scope, source_name, source_text)?;
    let mut resolved_scopes = Vec::new();
    for arm in scopes {
        let pattern = resolve_case_pattern(&arm.pattern, scope, source_name, source_text)?;
        let guard = scope.enter(ScopeKind::Case);
        let body = resolve_fn_body_inner(
            &arm.body,
            guard.stack,
            working_dir,
            source_name,
            source_text,
        )?;
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
    source_name: &str,
    source_text: &str,
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
                let resolved_value = resolve_expr(value, scope, source_name, source_text)?;
                let final_value = if *var_type == crate::dsl::VarType::Shell {
                    shell::execute_shell_variable(
                        name,
                        &resolved_value,
                        working_dir,
                        source_name,
                        source_text,
                        offset,
                        len,
                    )?
                } else {
                    resolved_value
                };
                scope
                    .declare(name.to_string(), final_value)
                    .map_err(|r| redeclaration_err(r, source_name, source_text, offset, len))?;
            }
            FnStmt::Log { value } => {
                let v = resolve_expr(value, scope, source_name, source_text)?;
                resolved.push(ResolvedFnStmt::Log { value: v });
            }
            FnStmt::Exec { value } => {
                let v = resolve_expr(value, scope, source_name, source_text)?;
                resolved.push(ResolvedFnStmt::Exec { value: v });
            }
            FnStmt::Cd { value } => {
                let v = resolve_expr(value, scope, source_name, source_text)?;
                resolved.push(ResolvedFnStmt::Cd { value: v });
            }
            FnStmt::EnvBlock { pairs, body } => {
                resolved.push(resolve_env_block_stmt(
                    pairs,
                    body,
                    scope,
                    working_dir,
                    source_name,
                    source_text,
                )?);
            }
            FnStmt::Case { condition, scopes } => {
                resolved.push(resolve_case_stmt(
                    condition,
                    scopes,
                    scope,
                    working_dir,
                    source_name,
                    source_text,
                )?);
            }
        }
    }
    Ok(resolved)
}
