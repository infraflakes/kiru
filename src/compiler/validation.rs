use crate::compiler::error::{CompileError, spanned_err};
use crate::dsl::{CasePattern, Expr, FnStmt};
use std::collections::{HashMap, HashSet};
use std::path::Path;

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
                errors.push("sanctuary declaration is required".to_string());
            }
            Some(Expr::VarRef { .. }) => {
                // Uses a variable reference — can't validate at this stage.
                // The resolve phase will catch undefined vars.
            }
            Some(Expr::BacktickLit { parts, .. }) => {
                let path_str: String = parts.iter().map(|part| part.value.as_str()).collect();
                if path_str.is_empty() {
                    errors.push("sanctuary declaration is required".to_string());
                } else if !Path::new(&path_str).is_absolute() {
                    errors.push(format!("sanctuary path must be absolute: {}", path_str));
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
                errors.push(format!("project {:?}: url is required", project.name));
            }

            if dir_str.is_empty() && project.dir.is_some() {
                // dir is set but uses var refs
            } else if project.dir.is_none() || dir_str.is_empty() {
                errors.push(format!("project {:?}: dir is required", project.name));
            }

            let normalized_dir = dir_str.trim_start_matches('/').to_string();
            if !normalized_dir.is_empty() && !seen_dirs.insert(normalized_dir) {
                errors.push(format!(
                    "project {:?}: duplicate directory {:?}",
                    project.name, dir_str
                ));
            }
        }
    }

    // Validate run references
    validate_run_refs(&cfg.runs, &cfg.functions, "top-level", &mut errors);

    // Build global scope set from pre-built scope
    let global_set: HashSet<String> = global_scope.keys().cloned().collect();

    validate_fn_bodies(
        &cfg.functions,
        &global_set,
        &HashSet::new(),
        "(top-level)",
        &mut errors,
    );

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

    if !errors.is_empty() {
        return Err(spanned_err(errors.join("\n"), "", "", 0, 1));
    }

    Ok(())
}

fn validate_run_refs(
    runs: &std::collections::HashMap<String, Vec<Vec<String>>>,
    functions: &std::collections::HashMap<String, Vec<FnStmt>>,
    prefix: &str,
    errors: &mut Vec<String>,
) {
    for (run_name, chains) in runs {
        for chain in chains {
            for fn_name in chain {
                if !functions.contains_key(fn_name) {
                    errors.push(format!(
                        "{}: run {:?} references unknown function {:?}",
                        prefix, run_name, fn_name
                    ));
                }
            }
        }
    }
}

fn validate_fn_bodies(
    functions: &std::collections::HashMap<String, Vec<FnStmt>>,
    global_scope: &HashSet<String>,
    project_scope: &HashSet<String>,
    proj_name: &str,
    errors: &mut Vec<String>,
) {
    for (fn_name, body) in functions {
        let mut mutable_scope: HashSet<String> =
            global_scope.union(project_scope).cloned().collect();
        validate_fn_body(fn_name, body, &mut mutable_scope, errors, proj_name);
    }
}

fn validate_expr(
    expr: &Expr,
    fn_name: &str,
    scope: &HashSet<String>,
    errors: &mut Vec<String>,
    proj_name: &str,
) {
    match expr {
        Expr::VarRef { name, .. } => {
            if !scope.contains(name) {
                errors.push(format!(
                    "project {:?}: fn {:?}: undefined variable ${}",
                    proj_name, fn_name, name
                ));
            }
        }
        Expr::BacktickLit { parts, .. } => {
            for part in parts {
                if part.is_var {
                    let var_name = part.value.trim_start_matches('$');
                    if !scope.contains(var_name) {
                        errors.push(format!(
                            "project {:?}: fn {:?}: undefined variable ${}",
                            proj_name, fn_name, var_name
                        ));
                    }
                }
            }
        }
    }
}

fn validate_fn_body(
    fn_name: &str,
    body: &[FnStmt],
    scope: &mut HashSet<String>,
    errors: &mut Vec<String>,
    proj_name: &str,
) {
    for stmt in body {
        match stmt {
            FnStmt::VarDecl { name, value, .. } => {
                validate_expr(value, fn_name, scope, errors, proj_name);
                scope.insert(name.clone());
            }
            FnStmt::Log { value, .. } => validate_expr(value, fn_name, scope, errors, proj_name),
            FnStmt::Exec { value, .. } => validate_expr(value, fn_name, scope, errors, proj_name),
            FnStmt::Cd { value, .. } => validate_expr(value, fn_name, scope, errors, proj_name),
            FnStmt::EnvBlock { pairs, body, .. } => {
                let mut block_scope = scope.clone();
                for pair in pairs {
                    validate_expr(&pair.value, fn_name, scope, errors, proj_name);
                }
                validate_fn_body(fn_name, body, &mut block_scope, errors, proj_name);
            }
            FnStmt::Case { condition, scopes } => {
                validate_expr(condition, fn_name, scope, errors, proj_name);
                for arm in scopes {
                    match &arm.pattern {
                        CasePattern::VarRef { name } => {
                            validate_expr(
                                &Expr::VarRef {
                                    name: name.clone(),
                                    offset: 0,
                                    len: 0,
                                },
                                fn_name,
                                scope,
                                errors,
                                proj_name,
                            );
                        }
                        CasePattern::Literal { parts } => {
                            for part in parts {
                                if part.is_var && !scope.contains(&part.value) {
                                    errors.push(format!(
                                        "project {:?}: fn {:?}: undefined variable ${}",
                                        proj_name, fn_name, part.value
                                    ));
                                }
                            }
                        }
                        CasePattern::Default => {}
                    }
                    let mut arm_scope = scope.clone();
                    validate_fn_body(fn_name, &arm.body, &mut arm_scope, errors, proj_name);
                }
            }
        }
    }
}
