use crate::config::error::ConfigError;
use crate::config::merge::merge_project_body_stmt;
use crate::config::types::Config;
use crate::dsl::ast::{CasePattern, Expr, FnStmt};
use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

pub(crate) fn validate_base(config: &Config) -> Result<(), ConfigError> {
    let mut errs = Vec::new();

    if config.shell.is_empty() {
        errs.push("shell declaration is required".to_string());
    }

    if config.sanctuary.is_empty() {
        errs.push("sanctuary declaration is required".to_string());
    } else if !Path::new(&config.sanctuary).is_absolute() {
        errs.push(format!(
            "sanctuary path must be absolute: {}",
            config.sanctuary
        ));
    }

    let mut dirs = std::collections::HashSet::new();
    for proj in config.projects.values() {
        if proj.url.is_empty() {
            errs.push(format!("project {:?}: url is required", proj.name));
        }
        if proj.dir.is_empty() {
            errs.push(format!("project {:?}: dir is required", proj.name));
        }
        if !dirs.insert(&proj.dir) {
            errs.push(format!(
                "project {:?}: duplicate directory {:?}",
                proj.name, proj.dir
            ));
        }
        match proj.sync.as_str() {
            "clone" | "ignore" => {}
            other => {
                errs.push(format!(
                    "project {:?}: invalid sync value {:?} (expected 'clone' or 'ignore')",
                    proj.name, other
                ));
            }
        }
    }

    if !errs.is_empty() {
        return Err(ConfigError::Validation(errs.join("\n")));
    }

    Ok(())
}

pub(crate) fn resolve_use(
    cfg: &mut Config,
    parse_recursive_fn: impl Fn(
        &Path,
        &mut HashSet<PathBuf>,
        &mut HashSet<PathBuf>,
    ) -> Result<Vec<crate::dsl::ast::Program>, ConfigError>,
) -> Result<(), ConfigError> {
    for proj in cfg.projects.values_mut() {
        let Some(use_file) = &proj.use_file else {
            continue;
        };

        if proj.sync == "ignore" {
            continue;
        }

        let use_path = PathBuf::from(&cfg.sanctuary).join(&proj.dir).join(use_file);

        if !use_path.exists() {
            return Err(ConfigError::Validation(format!(
                "project {:?}: use file not found: {} (run 'kiru sync' first)",
                proj.name,
                use_path.display()
            )));
        }

        let mut loaded_files = HashSet::new();
        let mut recursion_stack = HashSet::new();
        let programs = parse_recursive_fn(&use_path, &mut loaded_files, &mut recursion_stack)?;

        for program in &programs {
            for stmt in &program.stmts {
                merge_project_body_stmt(proj, stmt.clone(), &cfg.shell, &cfg.vars)?;
            }
        }
    }

    validate_full(cfg)?;

    Ok(())
}

fn validate_full(cfg: &Config) -> Result<(), ConfigError> {
    let mut errs = Vec::new();

    for (proj_name, project) in &cfg.projects {
        for (run_name, chains) in &project.runs {
            for chain in chains {
                for fn_name in chain {
                    if !project.functions.contains_key(fn_name) {
                        errs.push(format!(
                            "project {:?}: run {:?} references unknown function {:?}",
                            proj_name, run_name, fn_name
                        ));
                    }
                }
            }
        }

        validate_fn_vars(project, &cfg.vars, proj_name, &mut errs);
    }

    if !errs.is_empty() {
        return Err(ConfigError::Validation(errs.join("\n")));
    }

    Ok(())
}

fn validate_fn_vars(
    project: &crate::config::types::Project,
    global_vars: &HashMap<String, String>,
    proj_name: &str,
    errs: &mut Vec<String>,
) {
    fn validate_expr(
        expr: &Expr,
        fn_name: &str,
        scope: &HashSet<String>,
        errs: &mut Vec<String>,
        proj_name: &str,
    ) {
        match expr {
            Expr::VarRef { name, .. } => {
                if !scope.contains(name) {
                    errs.push(format!(
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
                            errs.push(format!(
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
        errs: &mut Vec<String>,
        proj_name: &str,
    ) {
        for stmt in body {
            match stmt {
                FnStmt::VarDecl { name, value, .. } => {
                    validate_expr(value, fn_name, scope, errs, proj_name);
                    if !scope.insert(name.clone()) {
                        errs.push(format!(
                            "project {:?}: fn {:?}: duplicate variable {:?}",
                            proj_name, fn_name, name
                        ));
                    }
                }
                FnStmt::Log { value, .. } => validate_expr(value, fn_name, scope, errs, proj_name),
                FnStmt::Exec { value, .. } => validate_expr(value, fn_name, scope, errs, proj_name),
                FnStmt::Cd { arg, .. } => validate_expr(arg, fn_name, scope, errs, proj_name),
                FnStmt::EnvBlock { pairs, body, .. } => {
                    let mut block_scope = scope.clone();
                    for pair in pairs {
                        validate_expr(&pair.value, fn_name, scope, errs, proj_name);
                        block_scope.insert(pair.key.clone());
                    }
                    validate_fn_body(fn_name, body, &mut block_scope, errs, proj_name);
                }
                FnStmt::Case { condition, arms } => {
                    validate_expr(condition, fn_name, scope, errs, proj_name);
                    for arm in arms {
                        match &arm.pattern {
                            CasePattern::VarRef { name } => {
                                validate_expr(
                                    &Expr::VarRef { name: name.clone() },
                                    fn_name,
                                    scope,
                                    errs,
                                    proj_name,
                                );
                            }
                            CasePattern::Literal { parts } => {
                                for part in parts {
                                    if part.is_var && !scope.contains(&part.value) {
                                        errs.push(format!(
                                            "project {:?}: fn {:?}: undefined variable ${}",
                                            proj_name, fn_name, part.value
                                        ));
                                    }
                                }
                            }
                            CasePattern::Default => {}
                        }
                        let mut arm_scope = scope.clone();
                        validate_fn_body(fn_name, &arm.body, &mut arm_scope, errs, proj_name);
                    }
                }
            }
        }
    }

    for (fn_name, body) in &project.functions {
        let mut scope: HashSet<String> = global_vars.keys().cloned().collect();
        scope.extend(project.vars.keys().cloned());
        validate_fn_body(fn_name, body, &mut scope, errs, proj_name);
    }
}
