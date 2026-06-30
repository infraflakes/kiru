use crate::compiler::error::CompileError;
use crate::dsl::{CasePattern, Expr, FnStmt};
use miette::miette;
use std::collections::{HashMap, HashSet};
use std::path::Path;

/// Check whether the `SANCTUARY` environment variable is set to `"0"`,
/// disabling sanctuary-mode validation.
pub fn is_sanctuary_disabled() -> bool {
    std::env::var("SANCTUARY").as_deref() == Ok("0")
}

/// Extract a plain string from an `Expr` if it is a simple backtick literal
/// with no variable interpolation. Returns `None` for var refs or interpolated
/// strings.
fn extract_string(expr: &Option<Expr>) -> Option<String> {
    let expr = expr.as_ref()?;
    match expr {
        Expr::BacktickLit { parts, .. } => {
            let mut extracted_string = String::new();
            for part in parts {
                if part.is_var {
                    return None;
                }
                extracted_string.push_str(&part.value);
            }
            Some(extracted_string)
        }
        Expr::VarRef { .. } => None,
    }
}

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

/// Validate an `UnresolvedSanctuary` against global and project scopes,
/// collecting all errors before returning.
pub fn validate_configuration(
    cfg: &super::types::UnresolvedSanctuary,
    global_scope: &HashMap<String, String>,
    project_var_scopes: &HashMap<String, HashMap<String, String>>,
) -> Result<(), CompileError> {
    let mut errors = Vec::new();

    // Sanity-check sanctuary path
    if is_sanctuary_disabled() {
        // SANCTUARY=0 mode: sanctuary and project fields are optional
    } else {
        match &cfg.sanctuary_path {
            None => {
                errors.push(miette!("sanctuary declaration is required"));
            }
            Some(Expr::VarRef { .. }) => {
                // Uses a variable reference — can't validate at this stage.
                // The resolve phase will catch undefined vars.
            }
            Some(Expr::BacktickLit { parts, .. }) => {
                let path_str: String = parts.iter().map(|part| part.value.as_str()).collect();
                if path_str.is_empty() {
                    errors.push(miette!("sanctuary declaration is required"));
                } else if !Path::new(&path_str).is_absolute() {
                    errors.push(miette!("sanctuary path must be absolute: {}", path_str));
                }
            }
        }
    }

    // Validate project fields (url/dir required, no duplicate dirs)
    if !is_sanctuary_disabled() {
        let mut seen_dirs = HashSet::<String>::new();
        for project in cfg.projects.values() {
            let url_str = extract_string(&project.url).unwrap_or_default();
            let dir_str = extract_string(&project.dir).unwrap_or_default();

            if url_str.is_empty() && project.url.is_some() {
                // url is set but uses var refs — can't validate now
            } else if project.url.is_none() || url_str.is_empty() {
                errors.push(miette!("project {:?}: url is required", project.name));
            }

            if dir_str.is_empty() && project.dir.is_some() {
                // dir is set but uses var refs
            } else if project.dir.is_none() || dir_str.is_empty() {
                errors.push(miette!("project {:?}: dir is required", project.name));
            }

            let normalized_dir = dir_str.trim_start_matches('/').to_string();
            if !normalized_dir.is_empty() && !seen_dirs.insert(normalized_dir) {
                errors.push(miette!(
                    "project {:?}: duplicate directory {:?}",
                    project.name,
                    dir_str
                ));
            }
        }
    }

    let global_set: HashSet<String> = global_scope.keys().cloned().collect();

    for (proj_name, project) in &cfg.projects {
        validate_run_refs(&project.runs, &project.functions, proj_name, &mut errors);

        // Build project scope set from the pre-built project scope
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
        // Use a scope stack with an initial empty frame for local vars.
        // Global and project scopes are referenced directly and never cloned.
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
                // Push a new scope frame so vars declared inside the env block
                // do not leak to the outer scope.
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
