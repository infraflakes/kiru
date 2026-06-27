use crate::compiler::error::CompileError;
use crate::compiler::error::spanned_err;
use crate::compiler::types::{
    Project, ResolvedCaseArm, ResolvedCasePattern, ResolvedEnvPair, ResolvedFnStmt, Sanctuary,
    SyncMode, UnresolvedSanctuary,
};
use crate::dsl::{CasePattern, Expr, FnStmt, Stmt, VarType};
use crate::shell;
use std::collections::HashMap;

/// Parse the sync mode string from a resolved value.
pub(crate) fn parse_sync_mode(value: &str) -> Result<SyncMode, String> {
    match value {
        "clone" => Ok(SyncMode::Clone),
        "ignore" => Ok(SyncMode::Ignore),
        _ => Err(format!(
            "invalid sync value {:?} (expected 'clone' or 'ignore')",
            value
        )),
    }
}

/// Resolve a single `Expr` against a scope (current vars).
fn resolve_expr_in_scope(
    expr: &Expr,
    scope: &HashMap<String, String>,
    source_name: &str,
    source_text: &str,
) -> Result<String, CompileError> {
    let err_for =
        |msg: String, o: usize, l: usize| spanned_err(msg, source_name, source_text, o, l);
    match expr {
        Expr::VarRef { name, offset, len } => {
            if let Some(val) = scope.get(name) {
                return Ok(val.clone());
            }
            Err(err_for(
                format!("undefined variable: ${}", name),
                *offset,
                *len,
            ))
        }
        Expr::BacktickLit { parts, offset, len } => {
            let mut result = String::new();
            for part in parts {
                if part.is_var {
                    if let Some(val) = scope.get(&part.value) {
                        result.push_str(val);
                    } else {
                        return Err(err_for(
                            format!("undefined variable: ${}", part.value),
                            *offset,
                            *len,
                        ));
                    }
                } else {
                    result.push_str(&part.value);
                }
            }
            Ok(result)
        }
    }
}

/// Execute a shell command to get its stdout, returning the output string.
/// Non-zero exit codes produce an empty string (see [`shell::exec_and_get_stdout`]).
pub(crate) fn exec_shell_var(
    name: &str,
    resolved_command: &str,
    source_name: &str,
    source_text: &str,
    offset: usize,
    len: usize,
) -> Result<String, CompileError> {
    match shell::exec_and_get_stdout(resolved_command, None, None) {
        Ok(stdout) => Ok(stdout),
        Err(shell::Error::Exit { .. }) => Ok(String::new()),
        Err(e) => Err(spanned_err(
            format!("shell var ${} failed: {}", name, e),
            source_name,
            source_text,
            offset,
            len,
        )),
    }
}

/// Resolve a `var` or `var shell` statement to a concrete string value.
pub(crate) fn resolve_var_stmt(
    stmt: &Stmt,
    scope: &mut HashMap<String, String>,
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
        let resolved = resolve_expr_in_scope(value, scope, source_name, source_text)?;
        let final_val = if *var_type == VarType::Shell {
            exec_shell_var(name, &resolved, source_name, source_text, *offset, *len)?
        } else {
            resolved
        };
        scope.insert(name.clone(), final_val);
    }
    Ok(())
}

/// Resolve an optional `Expr` field to a concrete string.
pub(crate) fn resolve_optional_expr(
    expr: &Option<Expr>,
    scope: &HashMap<String, String>,
    source_name: &str,
    source_text: &str,
) -> Result<Option<String>, CompileError> {
    match expr {
        Some(e) => {
            let resolved = resolve_expr_in_scope(e, scope, source_name, source_text)?;
            if resolved.is_empty() {
                Ok(None)
            } else {
                Ok(Some(resolved))
            }
        }
        None => Ok(None),
    }
}

/// Resolve using pre-computed scopes. Skips all var resolution loops and uses
/// the provided scopes directly. This avoids re-executing `var shell` commands
/// that were already evaluated during the linear processing phase.
pub(crate) fn resolve_with_scopes(
    unresolved: UnresolvedSanctuary,
    global_scope: HashMap<String, String>,
    mut project_scopes: HashMap<String, HashMap<String, String>>,
) -> Result<Sanctuary, CompileError> {
    let sanctuary_path = resolve_optional_expr(&unresolved.sanctuary_path, &global_scope, "", "")?
        .unwrap_or_default();

    let functions = resolve_fn_body_map(&unresolved.functions, &global_scope, &HashMap::new())?;

    let mut projects = HashMap::new();
    for (name, unresolved_project) in unresolved.projects {
        let proj_scope = project_scopes
            .remove(&name)
            .unwrap_or_else(|| global_scope.clone());

        let url = resolve_optional_expr(&unresolved_project.url, &proj_scope, "", "")?
            .unwrap_or_default();
        let dir = resolve_optional_expr(&unresolved_project.dir, &proj_scope, "", "")?
            .unwrap_or_default();

        let sync = match resolve_optional_expr(&unresolved_project.sync, &proj_scope, "", "")? {
            Some(mode) => parse_sync_mode(&mode).map_err(|msg| spanned_err(msg, "", "", 0, 1))?,
            None => SyncMode::Clone,
        };

        let branch = resolve_optional_expr(&unresolved_project.branch, &proj_scope, "", "")?;

        let proj_fns =
            resolve_fn_body_map(&unresolved_project.functions, &global_scope, &proj_scope)?;

        projects.insert(
            name,
            Project {
                url,
                dir,
                sync,
                branch,
                functions: proj_fns,
                runs: unresolved_project.runs,
            },
        );
    }

    Ok(Sanctuary {
        sanctuary_path,
        projects,
        functions,
        runs: unresolved.runs,
    })
}

/// Resolve an entire function body — all `Expr` nodes are substituted with
/// concrete `String` values, `var shell` commands are executed, and
/// `VarDecl` bindings are inlined and dropped from the output.
///
/// Resolution follows lexical scoping: local `var` declarations shadow
/// project vars, which shadow global vars in that order.
pub(crate) fn resolve_fn_body(
    body: &[FnStmt],
    global_vars: &HashMap<String, String>,
    project_vars: &HashMap<String, String>,
    source_name: &str,
    source_text: &str,
) -> Result<Vec<ResolvedFnStmt>, CompileError> {
    let mut scope: HashMap<String, String> = HashMap::new();
    scope.extend(
        global_vars
            .iter()
            .map(|(key, value)| (key.clone(), value.clone())),
    );
    scope.extend(project_vars.iter().map(|(k, v)| (k.clone(), v.clone())));
    resolve_fn_body_inner(body, &mut scope, source_name, source_text)
}

fn resolve_fn_body_inner(
    body: &[FnStmt],
    scope: &mut HashMap<String, String>,
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
                let resolved_value = resolve_expr_in_scope(value, scope, source_name, source_text)?;
                let (offset, len) = extract_expr_offset_len(value);
                let final_value = if *var_type == VarType::Shell {
                    exec_shell_var(name, &resolved_value, source_name, source_text, offset, len)?
                } else {
                    resolved_value
                };
                scope.insert(name.clone(), final_value);
            }
            FnStmt::Log { value } => {
                let resolved_value = resolve_expr_in_scope(value, scope, source_name, source_text)?;
                resolved.push(ResolvedFnStmt::Log {
                    value: resolved_value,
                });
            }
            FnStmt::Exec { value } => {
                let resolved_value = resolve_expr_in_scope(value, scope, source_name, source_text)?;
                resolved.push(ResolvedFnStmt::Exec {
                    value: resolved_value,
                });
            }
            FnStmt::Cd { value } => {
                let resolved_value = resolve_expr_in_scope(value, scope, source_name, source_text)?;
                resolved.push(ResolvedFnStmt::Cd {
                    value: resolved_value,
                });
            }
            FnStmt::EnvBlock { pairs, body } => {
                let mut resolved_pairs = Vec::new();
                for pair in pairs {
                    let resolved_value =
                        resolve_expr_in_scope(&pair.value, scope, source_name, source_text)?;
                    resolved_pairs.push(ResolvedEnvPair {
                        key: pair.key.clone(),
                        value: resolved_value,
                    });
                }
                let mut env_scope = scope.clone();
                let resolved_body =
                    resolve_fn_body_inner(body, &mut env_scope, source_name, source_text)?;
                resolved.push(ResolvedFnStmt::EnvBlock {
                    pairs: resolved_pairs,
                    body: resolved_body,
                });
            }
            FnStmt::Case { condition, scopes } => {
                let resolved_condition =
                    resolve_expr_in_scope(condition, scope, source_name, source_text)?;
                let mut resolved_scopes = Vec::new();
                for arm in scopes {
                    let pattern =
                        resolve_case_pattern(&arm.pattern, scope, source_name, source_text)?;
                    let mut arm_scope = scope.clone();
                    let body =
                        resolve_fn_body_inner(&arm.body, &mut arm_scope, source_name, source_text)?;
                    resolved_scopes.push(ResolvedCaseArm { pattern, body });
                }
                resolved.push(ResolvedFnStmt::Case {
                    condition: resolved_condition,
                    scopes: resolved_scopes,
                });
            }
        }
    }
    Ok(resolved)
}

/// Resolve a case pattern to its concrete form.
/// `VarRef` patterns are looked up in scope and flattened to `Literal`.
fn resolve_case_pattern(
    pattern: &CasePattern,
    scope: &HashMap<String, String>,
    source_name: &str,
    source_text: &str,
) -> Result<ResolvedCasePattern, CompileError> {
    match pattern {
        CasePattern::Literal { parts } => {
            let mut result = String::new();
            for part in parts {
                if part.is_var {
                    if let Some(val) = scope.get(&part.value) {
                        result.push_str(val);
                    } else {
                        return Err(spanned_err(
                            format!("undefined variable: ${}", part.value),
                            source_name,
                            source_text,
                            0,
                            1,
                        ));
                    }
                } else {
                    result.push_str(&part.value);
                }
            }
            Ok(ResolvedCasePattern::Literal(result))
        }
        CasePattern::VarRef { name } => {
            if let Some(val) = scope.get(name) {
                Ok(ResolvedCasePattern::Literal(val.clone()))
            } else {
                Err(spanned_err(
                    format!("undefined variable: ${}", name),
                    source_name,
                    source_text,
                    0,
                    1,
                ))
            }
        }
        CasePattern::Default => Ok(ResolvedCasePattern::Default),
    }
}

/// Extract (offset, len) from an `Expr` for error reporting in shell var execution.
fn extract_expr_offset_len(expr: &Expr) -> (usize, usize) {
    match expr {
        Expr::BacktickLit { offset, len, .. } => (*offset, *len),
        Expr::VarRef { offset, len, .. } => (*offset, *len),
    }
}

/// Resolve all functions in a function map (top-level or per-project).
fn resolve_fn_body_map(
    fns: &HashMap<String, Vec<FnStmt>>,
    global_vars: &HashMap<String, String>,
    project_vars: &HashMap<String, String>,
) -> Result<HashMap<String, Vec<ResolvedFnStmt>>, CompileError> {
    let mut resolved = HashMap::new();
    for (name, body) in fns {
        let resolved_body = resolve_fn_body(body, global_vars, project_vars, "", "")?;
        resolved.insert(name.clone(), resolved_body);
    }
    Ok(resolved)
}
