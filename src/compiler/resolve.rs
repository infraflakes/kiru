use crate::compiler::error::CompileError;
use crate::compiler::error::spanned_err;
use crate::compiler::types::{
    Project, ResolvedCaseArm, ResolvedCasePattern, ResolvedEnvPair, ResolvedFnStmt, Sanctuary,
    SyncMode, UnresolvedSanctuary,
};
use crate::dsl::{CasePattern, Expr, FnStmt, Stmt, VarType};
use crate::shell;
use std::collections::HashMap;

/// A chain of scope frames for lexical scoping in function bodies.
///
/// Each frame is a `HashMap<String, String>`. Lookups walk from the top
/// (innermost) frame down to the base (outermost) frame, returning the first
/// match. Inserts always go into the top frame.
///
/// This eliminates the need to clone entire HashMaps when entering branching
/// constructs like `env` blocks and `case` arms — instead we push/pop a new
/// empty frame, which is O(1). The cost is O(depth) lookups instead of O(1),
/// but depth is bounded by nesting (typically ≤5).
struct ScopeChain {
    frames: Vec<HashMap<String, String>>,
}

impl ScopeChain {
    fn from_base(base: HashMap<String, String>) -> Self {
        Self { frames: vec![base] }
    }

    fn get(&self, name: &str) -> Option<&String> {
        for frame in self.frames.iter().rev() {
            if let Some(val) = frame.get(name) {
                return Some(val);
            }
        }
        None
    }

    fn insert(&mut self, name: String, value: String) {
        if let Some(top) = self.frames.last_mut() {
            top.insert(name, value);
        }
    }

    fn push_frame(&mut self) {
        self.frames.push(HashMap::new());
    }

    fn pop_frame(&mut self) {
        self.frames.pop();
    }
}

/// Resolve an `Expr` against a scope chain. Mirrors `resolve_expr_in_scope`
/// but walks the chain for variable lookups.
fn resolve_expr_in_chain(
    expr: &Expr,
    chain: &ScopeChain,
    source_name: &str,
    source_text: &str,
) -> Result<String, CompileError> {
    let err_for =
        |msg: String, o: usize, l: usize| spanned_err(msg, source_name, source_text, o, l);
    match expr {
        Expr::VarRef { name, offset, len } => {
            if let Some(val) = chain.get(name) {
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
                    if let Some(val) = chain.get(&part.value) {
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

/// Resolve a case pattern against a scope chain.
fn resolve_case_pattern_in_chain(
    pattern: &CasePattern,
    chain: &ScopeChain,
    source_name: &str,
    source_text: &str,
) -> Result<ResolvedCasePattern, CompileError> {
    let err_for =
        |msg: String, o: usize, l: usize| spanned_err(msg, source_name, source_text, o, l);
    match pattern {
        CasePattern::Literal { parts, offset, len } => {
            let mut result = String::new();
            for part in parts {
                if part.is_var {
                    if let Some(val) = chain.get(&part.value) {
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
            Ok(ResolvedCasePattern::Literal(result))
        }
        CasePattern::VarRef { name, offset, len } => {
            if let Some(val) = chain.get(name) {
                Ok(ResolvedCasePattern::Literal(val.clone()))
            } else {
                Err(err_for(
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
            shell::execute_shell_variable(name, &resolved, source_name, source_text, *offset, *len)?
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
        let sync_offset_len = unresolved_project
            .sync
            .as_ref()
            .map(|e| match e {
                Expr::BacktickLit { offset, len, .. } => (*offset, *len),
                Expr::VarRef { offset, len, .. } => (*offset, *len),
            })
            .unwrap_or((0, 1));
        let proj_scope = project_scopes
            .remove(&name)
            .unwrap_or_else(|| global_scope.clone());

        let url = resolve_optional_expr(&unresolved_project.url, &proj_scope, "", "")?
            .unwrap_or_default();
        let dir = resolve_optional_expr(&unresolved_project.dir, &proj_scope, "", "")?
            .unwrap_or_default();

        let sync = match resolve_optional_expr(&unresolved_project.sync, &proj_scope, "", "")? {
            Some(mode) => {
                let (so, sl) = sync_offset_len;
                parse_sync_mode_value(&mode).map_err(|msg| spanned_err(msg, "", "", so, sl))?
            }
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
/// Resolution follows lexical scoping via a [`ScopeChain`]: local `var`
/// declarations shadow project vars, which shadow global vars in that order.
/// Branching constructs (`env` blocks, `case` arms) push/pop a new scope
/// frame instead of cloning the entire hash map.
pub(crate) fn resolve_fn_body(
    body: &[FnStmt],
    global_vars: &HashMap<String, String>,
    project_vars: &HashMap<String, String>,
    source_name: &str,
    source_text: &str,
) -> Result<Vec<ResolvedFnStmt>, CompileError> {
    let mut base: HashMap<String, String> = HashMap::new();
    base.extend(
        global_vars
            .iter()
            .map(|(key, value)| (key.clone(), value.clone())),
    );
    base.extend(project_vars.iter().map(|(k, v)| (k.clone(), v.clone())));
    let mut chain = ScopeChain::from_base(base);
    resolve_fn_body_inner(body, &mut chain, source_name, source_text)
}

fn resolve_fn_body_inner(
    body: &[FnStmt],
    chain: &mut ScopeChain,
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
                let resolved_value = resolve_expr_in_chain(value, chain, source_name, source_text)?;
                let (offset, len) = extract_expr_offset_len(value);
                let final_value = if *var_type == VarType::Shell {
                    shell::execute_shell_variable(
                        name,
                        &resolved_value,
                        source_name,
                        source_text,
                        offset,
                        len,
                    )?
                } else {
                    resolved_value
                };
                chain.insert(name.clone(), final_value);
            }
            FnStmt::Log { value } => {
                let resolved_value = resolve_expr_in_chain(value, chain, source_name, source_text)?;
                resolved.push(ResolvedFnStmt::Log {
                    value: resolved_value,
                });
            }
            FnStmt::Exec { value } => {
                let resolved_value = resolve_expr_in_chain(value, chain, source_name, source_text)?;
                resolved.push(ResolvedFnStmt::Exec {
                    value: resolved_value,
                });
            }
            FnStmt::Cd { value } => {
                let resolved_value = resolve_expr_in_chain(value, chain, source_name, source_text)?;
                resolved.push(ResolvedFnStmt::Cd {
                    value: resolved_value,
                });
            }
            FnStmt::EnvBlock { pairs, body } => {
                let mut resolved_pairs = Vec::new();
                for pair in pairs {
                    let resolved_value =
                        resolve_expr_in_chain(&pair.value, chain, source_name, source_text)?;
                    resolved_pairs.push(ResolvedEnvPair {
                        key: pair.key.clone(),
                        value: resolved_value,
                    });
                }
                chain.push_frame();
                let resolved_body = resolve_fn_body_inner(body, chain, source_name, source_text)?;
                chain.pop_frame();
                resolved.push(ResolvedFnStmt::EnvBlock {
                    pairs: resolved_pairs,
                    body: resolved_body,
                });
            }
            FnStmt::Case { condition, scopes } => {
                let resolved_condition =
                    resolve_expr_in_chain(condition, chain, source_name, source_text)?;
                let mut resolved_scopes = Vec::new();
                for arm in scopes {
                    let pattern = resolve_case_pattern_in_chain(
                        &arm.pattern,
                        chain,
                        source_name,
                        source_text,
                    )?;
                    chain.push_frame();
                    let body = resolve_fn_body_inner(&arm.body, chain, source_name, source_text)?;
                    chain.pop_frame();
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
