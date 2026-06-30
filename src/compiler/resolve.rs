use crate::compiler::error::CompileError;
use crate::compiler::error::spanned_err;
use crate::compiler::types::{
    Project, ResolvedCaseArm, ResolvedCasePattern, ResolvedEnvPair, ResolvedFnStmt, Sanctuary,
    SyncMode, UnresolvedSanctuary,
};
use crate::dsl::{CaseArm, CasePattern, EnvPair, Expr, FnStmt, Stmt, VarType};
use crate::shell;
use std::collections::HashMap;

/// A chain of scope frames for lexical scoping in function bodies.
///
/// Global and project-level variables are stored as references and never
/// cloned. Dynamic frames (for `env` blocks, `case` arms, and function-local
/// `var` declarations) are pushed/popped as needed.
///
/// Lookups walk from the innermost frame → project scope → global scope,
/// returning the first match. Inserts always go into the top frame.
struct ScopeChain<'a> {
    global: &'a HashMap<String, String>,
    project: &'a HashMap<String, String>,
    frames: Vec<HashMap<String, String>>,
}

impl<'a> ScopeChain<'a> {
    /// Create a new scope chain with references to global and project
    /// scopes, starting with one empty local frame.
    fn new(global: &'a HashMap<String, String>, project: &'a HashMap<String, String>) -> Self {
        Self {
            global,
            project,
            frames: vec![HashMap::new()],
        }
    }

    /// Look up a variable by name, searching from innermost frame outward
    /// to project scope, then global scope.
    fn get(&self, name: &str) -> Option<&String> {
        for frame in self.frames.iter().rev() {
            if let Some(val) = frame.get(name) {
                return Some(val);
            }
        }
        if let Some(val) = self.project.get(name) {
            return Some(val);
        }
        self.global.get(name)
    }

    /// Insert a variable into the top scope frame.
    fn insert(&mut self, name: String, value: String) {
        if let Some(top) = self.frames.last_mut() {
            top.insert(name, value);
        }
    }

    /// Push a new empty frame onto the scope stack.
    fn push_frame(&mut self) {
        self.frames.push(HashMap::new());
    }

    /// Pop the top frame from the scope stack.
    fn pop_frame(&mut self) {
        self.frames.pop();
    }
}

/// Resolve an `Expr` against a scope chain. Mirrors `resolve_expr_in_scope`
/// but walks the chain for variable lookups.
fn resolve_expr_in_chain(
    expr: &Expr,
    chain: &ScopeChain<'_>,
    source_name: &str,
    source_text: &str,
) -> Result<String, CompileError> {
    let make_span_error =
        |msg: String, o: usize, l: usize| spanned_err(msg, source_name, source_text, o, l);
    match expr {
        Expr::VarRef { name, offset, len } => {
            if let Some(val) = chain.get(name) {
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
                    if let Some(val) = chain.get(&part.value) {
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

/// Resolve a case pattern against a scope chain.
fn resolve_case_pattern_in_chain(
    pattern: &CasePattern,
    chain: &ScopeChain<'_>,
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
                    if let Some(val) = chain.get(&part.value) {
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
            if let Some(val) = chain.get(name) {
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

/// Resolve a single `Expr` against a scope (current vars).
fn resolve_expr_in_scope(
    expr: &Expr,
    scope: &HashMap<String, String>,
    source_name: &str,
    source_text: &str,
) -> Result<String, CompileError> {
    let make_span_error =
        |msg: String, o: usize, l: usize| spanned_err(msg, source_name, source_text, o, l);
    match expr {
        Expr::VarRef { name, offset, len } => {
            if let Some(val) = scope.get(name) {
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
                    if let Some(val) = scope.get(&part.value) {
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
    project_scopes: HashMap<String, HashMap<String, String>>,
) -> Result<Sanctuary, CompileError> {
    let sanctuary_path = resolve_optional_expr(&unresolved.sanctuary_path, &global_scope, "", "")?
        .unwrap_or_default();

    let mut projects = HashMap::new();
    for (name, unresolved_project) in unresolved.projects {
        let sync_offset_len = unresolved_project
            .sync
            .as_ref()
            .map(|e| e.offset_len())
            .unwrap_or((0, 1));
        let proj_scope: &HashMap<String, String> =
            project_scopes.get(&name).unwrap_or(&global_scope);

        let url =
            resolve_optional_expr(&unresolved_project.url, proj_scope, "", "")?.unwrap_or_default();
        let dir =
            resolve_optional_expr(&unresolved_project.dir, proj_scope, "", "")?.unwrap_or_default();

        let sync = match resolve_optional_expr(&unresolved_project.sync, proj_scope, "", "")? {
            Some(mode) => {
                let (sync_offset, sync_len) = sync_offset_len;
                parse_sync_mode_value(&mode)
                    .map_err(|msg| spanned_err(msg, "", "", sync_offset, sync_len))?
            }
            None => SyncMode::Clone,
        };

        let branch = resolve_optional_expr(&unresolved_project.branch, proj_scope, "", "")?;

        let proj_fns =
            resolve_fn_body_map(&unresolved_project.functions, &global_scope, proj_scope)?;

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
    let mut chain = ScopeChain::new(global_vars, project_vars);
    resolve_fn_body_inner(body, &mut chain, source_name, source_text)
}

/// Resolve a variable declaration, executing shell vars if needed, and
/// insert the result into the scope chain.
fn resolve_var_decl_stmt(
    var_type: &VarType,
    name: &str,
    value: &Expr,
    chain: &mut ScopeChain<'_>,
    source_name: &str,
    source_text: &str,
) -> Result<(), CompileError> {
    let resolved_value = resolve_expr_in_chain(value, chain, source_name, source_text)?;
    let (offset, len) = extract_expr_offset_len(value);
    let final_value = if *var_type == VarType::Shell {
        shell::execute_shell_variable(name, &resolved_value, source_name, source_text, offset, len)?
    } else {
        resolved_value
    };
    chain.insert(name.to_string(), final_value);
    Ok(())
}

/// Resolve the value expression in a log statement to a concrete string.
fn resolve_log_stmt(
    value: &Expr,
    chain: &mut ScopeChain<'_>,
    source_name: &str,
    source_text: &str,
) -> Result<ResolvedFnStmt, CompileError> {
    let resolved_value = resolve_expr_in_chain(value, chain, source_name, source_text)?;
    Ok(ResolvedFnStmt::Log {
        value: resolved_value,
    })
}

/// Resolve the value expression in an exec statement to a concrete string.
fn resolve_exec_stmt(
    value: &Expr,
    chain: &mut ScopeChain<'_>,
    source_name: &str,
    source_text: &str,
) -> Result<ResolvedFnStmt, CompileError> {
    let resolved_value = resolve_expr_in_chain(value, chain, source_name, source_text)?;
    Ok(ResolvedFnStmt::Exec {
        value: resolved_value,
    })
}

/// Resolve the value expression in a cd statement to a concrete string.
fn resolve_cd_stmt(
    value: &Expr,
    chain: &mut ScopeChain<'_>,
    source_name: &str,
    source_text: &str,
) -> Result<ResolvedFnStmt, CompileError> {
    let resolved_value = resolve_expr_in_chain(value, chain, source_name, source_text)?;
    Ok(ResolvedFnStmt::Cd {
        value: resolved_value,
    })
}

/// Resolve an env block: resolve each pair's value, push a scope frame,
/// resolve the body, then pop the frame.
fn resolve_env_block_stmt(
    pairs: &[EnvPair],
    body: &[FnStmt],
    chain: &mut ScopeChain<'_>,
    source_name: &str,
    source_text: &str,
) -> Result<ResolvedFnStmt, CompileError> {
    let mut resolved_pairs = Vec::new();
    for pair in pairs {
        let resolved_value = resolve_expr_in_chain(&pair.value, chain, source_name, source_text)?;
        resolved_pairs.push(ResolvedEnvPair {
            key: pair.key.clone(),
            value: resolved_value,
        });
    }
    chain.push_frame();
    let resolved_body = resolve_fn_body_inner(body, chain, source_name, source_text)?;
    chain.pop_frame();
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
    chain: &mut ScopeChain<'_>,
    source_name: &str,
    source_text: &str,
) -> Result<ResolvedFnStmt, CompileError> {
    let resolved_condition = resolve_expr_in_chain(condition, chain, source_name, source_text)?;
    let mut resolved_scopes = Vec::new();
    for arm in scopes {
        let pattern = resolve_case_pattern_in_chain(&arm.pattern, chain, source_name, source_text)?;
        chain.push_frame();
        let body = resolve_fn_body_inner(&arm.body, chain, source_name, source_text)?;
        chain.pop_frame();
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
    chain: &mut ScopeChain<'_>,
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
                resolve_var_decl_stmt(var_type, name, value, chain, source_name, source_text)?;
            }
            FnStmt::Log { value } => {
                resolved.push(resolve_log_stmt(value, chain, source_name, source_text)?);
            }
            FnStmt::Exec { value } => {
                resolved.push(resolve_exec_stmt(value, chain, source_name, source_text)?);
            }
            FnStmt::Cd { value } => {
                resolved.push(resolve_cd_stmt(value, chain, source_name, source_text)?);
            }
            FnStmt::EnvBlock { pairs, body } => {
                resolved.push(resolve_env_block_stmt(
                    pairs,
                    body,
                    chain,
                    source_name,
                    source_text,
                )?);
            }
            FnStmt::Case { condition, scopes } => {
                resolved.push(resolve_case_stmt(
                    condition,
                    scopes,
                    chain,
                    source_name,
                    source_text,
                )?);
            }
        }
    }
    Ok(resolved)
}

/// Extract (offset, len) from an `Expr` for error reporting in shell var execution.
fn extract_expr_offset_len(expr: &Expr) -> (usize, usize) {
    expr.offset_len()
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
