use crate::compiler::error::CompileError;
use crate::dsl::{CasePattern, Expr, FnStmt};
use miette::miette;
use std::collections::{HashMap, HashSet};

/// Check whether a variable name is defined across the full scope hierarchy:
/// local frames (innermost first), then project scope, then global scope.
fn is_var_defined(
    name: &str,
    global_scope: &HashSet<String>,
    project_scope: &HashSet<String>,
    local: &[HashSet<String>],
) -> bool {
    for frame in local.iter().rev() {
        if frame.contains(name) {
            return true;
        }
    }
    project_scope.contains(name) || global_scope.contains(name)
}

/// Validate an `UnresolvedConfig` against global and project scopes,
/// collecting all errors before returning.
pub fn validate_configuration(
    cfg: &super::types::UnresolvedConfig,
    global_scope: &HashMap<String, String>,
    project_var_scopes: &HashMap<String, HashMap<String, String>>,
) -> Result<(), CompileError> {
    let mut errors = Vec::new();

    let global_set: HashSet<String> = global_scope.keys().cloned().collect();

    for (proj_name, project) in &cfg.projects {
        validate_run_refs(&project.runs, &project.functions, proj_name, &mut errors);

        let project_set: HashSet<String> = project_var_scopes
            .get(proj_name)
            .map(|m| m.keys().cloned().collect())
            .unwrap_or_default();

        validate_fn_bodies(
            &project.functions,
            &global_set,
            &project_set,
            proj_name,
            &mut errors,
        );
    }

    if errors.len() == 1 {
        return Err(CompileError::ValidationReport(
            errors.into_iter().next().unwrap(),
        ));
    } else if !errors.is_empty() {
        let mut combined = String::new();
        for (i, report) in errors.iter().enumerate() {
            if i > 0 {
                combined.push('\n');
            }
            combined.push_str(&format!("{}", report));
        }
        return Err(CompileError::ValidationReport(miette!(
            "{}\n{} validation error(s) found",
            combined,
            errors.len()
        )));
    }

    Ok(())
}

/// Check that all run chains reference functions that exist in the
/// project's function map.
fn validate_run_refs(
    runs: &std::collections::HashMap<String, Vec<Vec<String>>>,
    functions: &std::collections::HashMap<String, Vec<FnStmt>>,
    prefix: &str,
    errors: &mut Vec<miette::Report>,
) {
    for (run_name, chains) in runs {
        for chain in chains {
            for fn_name in chain {
                if !functions.contains_key(fn_name) {
                    errors.push(miette!(
                        "{}: run {:?} references unknown function {:?}",
                        prefix,
                        run_name,
                        fn_name
                    ));
                }
            }
        }
    }
}

/// Validate all function bodies in a project's function map, using a
/// fresh scope stack for each function.
fn validate_fn_bodies(
    functions: &std::collections::HashMap<String, Vec<FnStmt>>,
    global_scope: &HashSet<String>,
    project_scope: &HashSet<String>,
    proj_name: &str,
    errors: &mut Vec<miette::Report>,
) {
    for (fn_name, body) in functions {
        let mut local: Vec<HashSet<String>> = vec![HashSet::new()];
        validate_fn_body(
            fn_name,
            body,
            global_scope,
            project_scope,
            &mut local,
            errors,
            proj_name,
        );
    }
}

/// Check that all variable references in an `Expr` are defined in the
/// current scope hierarchy.
fn validate_expr(
    expr: &Expr,
    fn_name: &str,
    global_scope: &HashSet<String>,
    project_scope: &HashSet<String>,
    local: &[HashSet<String>],
    errors: &mut Vec<miette::Report>,
    proj_name: &str,
) {
    match expr {
        Expr::VarRef { name, .. } => {
            if !is_var_defined(name, global_scope, project_scope, local) {
                errors.push(miette!(
                    "project {:?}: fn {:?}: undefined variable ${}",
                    proj_name,
                    fn_name,
                    name
                ));
            }
        }
        Expr::BacktickLit { parts, .. } => {
            for part in parts {
                if part.is_var {
                    let var_name = part.value.trim_start_matches('$');
                    if !is_var_defined(var_name, global_scope, project_scope, local) {
                        errors.push(miette!(
                            "project {:?}: fn {:?}: undefined variable ${}",
                            proj_name,
                            fn_name,
                            var_name
                        ));
                    }
                }
            }
        }
    }
}

/// Validate variable references in a function body, tracking local
/// declarations in a scope stack.
fn validate_fn_body(
    fn_name: &str,
    body: &[FnStmt],
    global_scope: &HashSet<String>,
    project_scope: &HashSet<String>,
    local: &mut Vec<HashSet<String>>,
    errors: &mut Vec<miette::Report>,
    proj_name: &str,
) {
    for stmt in body {
        match stmt {
            FnStmt::VarDecl { name, value, .. } => {
                validate_expr(
                    value,
                    fn_name,
                    global_scope,
                    project_scope,
                    local,
                    errors,
                    proj_name,
                );
                if let Some(top) = local.last_mut() {
                    top.insert(name.clone());
                }
            }
            FnStmt::Log { value, .. } => {
                validate_expr(
                    value,
                    fn_name,
                    global_scope,
                    project_scope,
                    local,
                    errors,
                    proj_name,
                );
            }
            FnStmt::Exec { value, .. } => {
                validate_expr(
                    value,
                    fn_name,
                    global_scope,
                    project_scope,
                    local,
                    errors,
                    proj_name,
                );
            }
            FnStmt::Cd { value, .. } => {
                validate_expr(
                    value,
                    fn_name,
                    global_scope,
                    project_scope,
                    local,
                    errors,
                    proj_name,
                );
            }
            FnStmt::EnvBlock { pairs, body, .. } => {
                for pair in pairs {
                    validate_expr(
                        &pair.value,
                        fn_name,
                        global_scope,
                        project_scope,
                        local,
                        errors,
                        proj_name,
                    );
                }
                local.push(HashSet::new());
                validate_fn_body(
                    fn_name,
                    body,
                    global_scope,
                    project_scope,
                    local,
                    errors,
                    proj_name,
                );
                local.pop();
            }
            FnStmt::Case { condition, scopes } => {
                validate_expr(
                    condition,
                    fn_name,
                    global_scope,
                    project_scope,
                    local,
                    errors,
                    proj_name,
                );
                for arm in scopes {
                    match &arm.pattern {
                        CasePattern::VarRef { name, .. } => {
                            validate_expr(
                                &Expr::VarRef {
                                    name: name.clone(),
                                    offset: 0,
                                    len: 0,
                                },
                                fn_name,
                                global_scope,
                                project_scope,
                                local,
                                errors,
                                proj_name,
                            );
                        }
                        CasePattern::Literal { parts, .. } => {
                            for part in parts {
                                if part.is_var
                                    && !is_var_defined(
                                        &part.value,
                                        global_scope,
                                        project_scope,
                                        local,
                                    )
                                {
                                    errors.push(miette!(
                                        "project {:?}: fn {:?}: undefined variable ${}",
                                        proj_name,
                                        fn_name,
                                        part.value
                                    ));
                                }
                            }
                        }
                        CasePattern::Default => {}
                    }
                    local.push(HashSet::new());
                    validate_fn_body(
                        fn_name,
                        &arm.body,
                        global_scope,
                        project_scope,
                        local,
                        errors,
                        proj_name,
                    );
                    local.pop();
                }
            }
        }
    }
}
