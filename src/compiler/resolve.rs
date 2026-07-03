use crate::compiler::error::CompileError;
use crate::compiler::error::spanned_err;
use crate::compiler::types::{
    Config, Project, ResolvedCaseArm, ResolvedCasePattern, ResolvedEnvPair, ResolvedFnStmt,
    SyncMode, UnresolvedConfig, UnresolvedProject,
};
use crate::dsl::{CaseArm, CasePattern, EnvPair, Expr, FnStmt, Stmt, VarType};
use crate::shell;
use miette::miette;
use std::collections::{HashMap, HashSet};
use std::path::Path;

/// Look up a variable name across local case-arm frames (innermost first),
/// then the flat var scope.
fn lookup_var<'a>(
    name: &str,
    vars: &'a HashMap<String, String>,
    local: &'a [HashMap<String, String>],
) -> Option<&'a String> {
    for frame in local.iter().rev() {
        if let Some(val) = frame.get(name) {
            return Some(val);
        }
    }
    vars.get(name)
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

/// Resolve an `Expr` to a concrete string. Looks up `$var` references
/// in local case-arm frames first, then the flat var scope.
pub(crate) fn resolve_expr(
    expr: &Expr,
    vars: &HashMap<String, String>,
    local: &[HashMap<String, String>],
    source_name: &str,
    source_text: &str,
) -> Result<String, CompileError> {
    let make_span_error =
        |msg: String, o: usize, l: usize| spanned_err(msg, source_name, source_text, o, l);
    match expr {
        Expr::VarRef { name, offset, len } => {
            if let Some(val) = lookup_var(name, vars, local) {
                return Ok(val.clone());
            }
            Err(make_span_error(
                format!("undefined variable: ${}", name),
                *offset,
                *len,
            ))
        }
        Expr::BacktickLit { parts, offset, len } => {
            let mut result = String::new();
            for part in parts {
                if part.is_var {
                    if let Some(val) = lookup_var(&part.value, vars, local) {
                        result.push_str(val);
                    } else {
                        return Err(make_span_error(
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

/// Resolve a case pattern against vars + local frames.
fn resolve_case_pattern(
    pattern: &CasePattern,
    vars: &HashMap<String, String>,
    local: &[HashMap<String, String>],
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
                    if let Some(val) = lookup_var(&part.value, vars, local) {
                        result.push_str(val);
                    } else {
                        return Err(make_span_error(
                            format!("undefined variable: ${}", part.value),
                            *offset,
                            *len,
                        ));
                    }
                } else {
                    result.push_str(&part.value);
                }
            }
            Ok(ResolvedCasePattern::Literal(result))
        }
        CasePattern::VarRef { name, offset, len } => {
            if let Some(val) = lookup_var(name, vars, local) {
                Ok(ResolvedCasePattern::Literal(val.clone()))
            } else {
                Err(make_span_error(
                    format!("undefined variable: ${}", name),
                    *offset,
                    *len,
                ))
            }
        }
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

/// Resolve and bind a `var` or `var shell` from a `Stmt::Var` (used during
/// linear processing — no case-arm local frames exist at this stage).
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
        let resolved = resolve_expr(value, &*scope, &[], source_name, source_text)?;
        let final_val = if *var_type == VarType::Shell {
            shell::execute_shell_variable(name, &resolved, source_name, source_text, *offset, *len)?
        } else {
            resolved
        };
        if scope.contains_key(name) {
            return Err(spanned_err(
                format!("${} is already defined", name),
                source_name,
                source_text,
                *offset,
                *len,
            ));
        }
        scope.insert(name.clone(), final_val);
    }
    Ok(())
}

/// Resolve and bind a `var` or `var shell` declaration inside a function body.
///
/// When inside a case arm (`local` is non-empty), the variable is bound into
/// the top local frame only — it is invisible outside that arm and does not
/// participate in the global uniqueness check.  When outside any case arm,
/// the variable is bound into the flat var scope and checked for duplicates.
pub(crate) fn resolve_var_decl_stmt(
    var_type: &VarType,
    name: &str,
    value: &Expr,
    vars: &mut HashMap<String, String>,
    local: &mut Vec<HashMap<String, String>>,
    source_name: &str,
    source_text: &str,
) -> Result<(), CompileError> {
    let resolved_value = resolve_expr(value, &*vars, &*local, source_name, source_text)?;
    let (offset, len) = value.offset_len();
    let final_value = if *var_type == VarType::Shell {
        shell::execute_shell_variable(name, &resolved_value, source_name, source_text, offset, len)?
    } else {
        resolved_value
    };

    if local.is_empty() {
        // Outside any case arm — bind into the flat global pool.
        if vars.contains_key(name) {
            return Err(spanned_err(
                format!("${} is already defined", name),
                source_name,
                source_text,
                offset,
                len,
            ));
        }
        vars.insert(name.to_string(), final_value);
    } else {
        // Inside a case arm — bind into the arm-local frame only.
        let top = local.last_mut().ok_or_else(|| {
            spanned_err(
                "internal error: empty local frame in case arm variable declaration".to_string(),
                source_name,
                source_text,
                offset,
                len,
            )
        })?;
        if top.contains_key(name) {
            return Err(spanned_err(
                format!("${} is already defined in this case arm", name),
                source_name,
                source_text,
                offset,
                len,
            ));
        }
        top.insert(name.to_string(), final_value);
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
            let resolved = resolve_expr(e, scope, &[], source_name, source_text)?;
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
    var_scope: &HashMap<String, String>,
) -> Result<String, CompileError> {
    let raw = resolve_optional_expr(&unresolved.dir, var_scope, "", "")?.unwrap_or_default();
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

/// Resolve an unresolved project's field expressions against the var scope.
pub(crate) fn resolve_project_fields(
    unresolved: &UnresolvedProject,
    var_scope: &HashMap<String, String>,
) -> Result<(String, String, SyncMode, Option<String>), CompileError> {
    let sync_offset_len = unresolved
        .sync
        .as_ref()
        .map(|e| e.offset_len())
        .unwrap_or((0, 1));
    let url = resolve_optional_expr(&unresolved.url, var_scope, "", "")?.unwrap_or_default();
    let dir = resolve_dir_field(unresolved, var_scope)?;
    let sync = match resolve_optional_expr(&unresolved.sync, var_scope, "", "")? {
        Some(mode) => {
            let (sync_offset, sync_len) = sync_offset_len;
            parse_sync_mode_value(&mode)
                .map_err(|msg| spanned_err(msg, "", "", sync_offset, sync_len))?
        }
        None => SyncMode::Clone,
    };
    let branch = resolve_optional_expr(&unresolved.branch, var_scope, "", "")?;
    Ok((url, dir, sync, branch))
}

/// Resolve using pre-computed scopes.
pub(crate) fn resolve_with_scopes(
    unresolved: UnresolvedConfig,
    mut var_scope: HashMap<String, String>,
) -> Result<Config, CompileError> {
    let mut projects = HashMap::new();
    let mut seen_dirs: HashSet<String> = HashSet::new();
    for (name, unresolved_project) in unresolved.projects {
        let (url, dir, sync, branch) = resolve_project_fields(&unresolved_project, &var_scope)?;

        if !dir.is_empty() && !seen_dirs.insert(dir.clone()) {
            return Err(CompileError::ValidationReport(miette!(
                "project {:?}: duplicate directory {:?}",
                name,
                dir
            )));
        }

        // Resolve function bodies against the flat var scope.
        let mut functions = HashMap::new();
        for (fn_name, body) in &unresolved_project.functions {
            let mut local = Vec::new();
            let resolved_body = resolve_fn_body_inner(body, &mut var_scope, &mut local, "", "")?;
            functions.insert(fn_name.clone(), resolved_body);
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
    vars: &mut HashMap<String, String>,
    local: &mut Vec<HashMap<String, String>>,
    source_name: &str,
    source_text: &str,
) -> Result<ResolvedFnStmt, CompileError> {
    let mut resolved_pairs = Vec::new();
    for pair in pairs {
        let resolved_value = resolve_expr(&pair.value, &*vars, &*local, source_name, source_text)?;
        resolved_pairs.push(ResolvedEnvPair {
            key: pair.key.clone(),
            value: resolved_value,
        });
    }
    let resolved_body = resolve_fn_body_inner(body, vars, local, source_name, source_text)?;
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
    vars: &mut HashMap<String, String>,
    local: &mut Vec<HashMap<String, String>>,
    source_name: &str,
    source_text: &str,
) -> Result<ResolvedFnStmt, CompileError> {
    let resolved_condition = resolve_expr(condition, &*vars, &*local, source_name, source_text)?;
    let mut resolved_scopes = Vec::new();
    for arm in scopes {
        let pattern =
            resolve_case_pattern(&arm.pattern, &*vars, &*local, source_name, source_text)?;
        local.push(HashMap::new());
        let body = resolve_fn_body_inner(&arm.body, vars, local, source_name, source_text)?;
        local.pop();
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
    vars: &mut HashMap<String, String>,
    local: &mut Vec<HashMap<String, String>>,
    source_name: &str,
    source_text: &str,
) -> Result<Vec<ResolvedFnStmt>, CompileError> {
    let mut resolved = Vec::new();
    for stmt in body {
        match stmt {
            // VarDecl is consumed at compile time — no ResolvedFnStmt produced.
            FnStmt::VarDecl {
                var_type,
                name,
                value,
            } => {
                resolve_var_decl_stmt(
                    var_type,
                    name,
                    value,
                    vars,
                    local,
                    source_name,
                    source_text,
                )?;
            }
            FnStmt::Log { value } => {
                let v = resolve_expr(value, &*vars, &*local, source_name, source_text)?;
                resolved.push(ResolvedFnStmt::Log { value: v });
            }
            FnStmt::Exec { value } => {
                let v = resolve_expr(value, &*vars, &*local, source_name, source_text)?;
                resolved.push(ResolvedFnStmt::Exec { value: v });
            }
            FnStmt::Cd { value } => {
                let v = resolve_expr(value, &*vars, &*local, source_name, source_text)?;
                resolved.push(ResolvedFnStmt::Cd { value: v });
            }
            FnStmt::EnvBlock { pairs, body } => {
                resolved.push(resolve_env_block_stmt(
                    pairs,
                    body,
                    vars,
                    local,
                    source_name,
                    source_text,
                )?);
            }
            FnStmt::Case { condition, scopes } => {
                resolved.push(resolve_case_stmt(
                    condition,
                    scopes,
                    vars,
                    local,
                    source_name,
                    source_text,
                )?);
            }
        }
    }
    Ok(resolved)
}
