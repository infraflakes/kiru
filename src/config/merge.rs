use crate::config::error::ConfigError;
use crate::config::types::{Config, Project};
use crate::dsl::ast::{Expr, Program, Stmt, VarType};
use crate::shell;
use std::collections::HashMap;

pub(crate) fn merge(programs: Vec<Program>) -> Result<Config, ConfigError> {
    let shell = collect_shell(&programs)?;
    let global_vars = collect_global_vars(&programs, &shell)?;
    let (sanctuary_expr, projects) = collect_projects(programs, &global_vars, &shell)?;

    let sanctuary = match sanctuary_expr {
        Some(ref expr) => expr
            .resolve(&global_vars)
            .map_err(ConfigError::Validation)?,
        None => String::new(),
    };

    Ok(Config {
        shell,
        sanctuary,
        projects,
        vars: global_vars,
    })
}

fn collect_shell(programs: &[Program]) -> Result<String, ConfigError> {
    let mut shell = String::new();
    for program in programs {
        for stmt in &program.stmts {
            if let Stmt::ShellDecl { value } = stmt {
                if !shell.is_empty() {
                    return Err(ConfigError::Validation(
                        "duplicate shell declaration".to_string(),
                    ));
                }
                shell = value.clone();
            }
        }
    }
    Ok(shell)
}

fn resolve_shell_var(shell: &str, resolved: &str) -> Result<String, ConfigError> {
    if shell.is_empty() {
        return Err(ConfigError::Validation(
            "shell must be declared before using shell variables".to_string(),
        ));
    }
    let out = shell::run_captured(
        shell,
        resolved,
        None,
        None,
        Some(std::time::Duration::from_secs(30)),
    )
    .map_err(|e| ConfigError::Validation(e.to_string()))?;
    Ok(out.stdout)
}

fn collect_global_vars(
    programs: &[Program],
    shell: &str,
) -> Result<HashMap<String, String>, ConfigError> {
    let mut global_vars = HashMap::new();
    for program in programs {
        for stmt in &program.stmts {
            if let Stmt::VarDecl {
                name,
                value,
                var_type,
            } = stmt
            {
                if global_vars.contains_key(name) {
                    return Err(ConfigError::Validation(format!(
                        "duplicate variable: {}",
                        name
                    )));
                }

                let resolved = value
                    .resolve(&global_vars)
                    .map_err(ConfigError::Validation)?;

                let final_value = if var_type == &VarType::Shell {
                    resolve_shell_var(shell, &resolved)?
                } else {
                    resolved
                };

                global_vars.insert(name.clone(), final_value);
            }
        }
    }
    Ok(global_vars)
}

fn collect_projects(
    programs: Vec<Program>,
    global_vars: &HashMap<String, String>,
    shell: &str,
) -> Result<(Option<Expr>, HashMap<String, Project>), ConfigError> {
    let mut sanctuary_expr: Option<Expr> = None;
    let mut projects: HashMap<String, Project> = HashMap::new();

    for program in programs {
        for stmt in program.stmts {
            match stmt {
                Stmt::ShellDecl { .. } => {}
                Stmt::SanctuaryDecl { value } => {
                    if sanctuary_expr.is_some() {
                        return Err(ConfigError::Validation(
                            "duplicate sanctuary declaration".to_string(),
                        ));
                    }
                    sanctuary_expr = Some(value);
                }
                Stmt::ProjectDecl {
                    name, fields, body, ..
                } => {
                    if projects.contains_key(&name) {
                        return Err(ConfigError::Validation(format!(
                            "duplicate project: {}",
                            name
                        )));
                    }

                    let mut project = Project {
                        name: name.clone(),
                        url: String::new(),
                        dir: String::new(),
                        sync: "clone".to_string(),
                        include_file: None,
                        branch: String::new(),
                        vars: HashMap::new(),
                        functions: HashMap::new(),
                        runs: HashMap::new(),
                    };

                    for field in &fields {
                        let value = field
                            .value
                            .resolve(global_vars)
                            .map_err(ConfigError::Validation)?;
                        match field.key.as_str() {
                            "url" => project.url = value,
                            "dir" => project.dir = value,
                            "sync" => project.sync = value,
                            "include" => {
                                if !value.is_empty() {
                                    project.include_file = Some(value);
                                }
                            }
                            "branch" => project.branch = value,
                            _ => {
                                return Err(ConfigError::Validation(format!(
                                    "unknown project field: {}",
                                    field.key
                                )));
                            }
                        }
                    }

                    let mut merged_vars = global_vars.clone();
                    for body_stmt in body {
                        merge_project_body_stmt(&mut project, body_stmt, shell, &mut merged_vars)?;
                    }

                    projects.insert(name, project);
                }
                _ => {}
            }
        }
    }

    Ok((sanctuary_expr, projects))
}

pub(crate) fn merge_project_body_stmt(
    project: &mut Project,
    stmt: Stmt,
    shell: &str,
    merged: &mut HashMap<String, String>,
) -> Result<(), ConfigError> {
    match stmt {
        Stmt::VarDecl {
            name,
            value,
            var_type,
        } => {
            if project.vars.contains_key(&name) {
                return Err(ConfigError::Validation(format!(
                    "duplicate variable in project '{}': {}",
                    project.name, name
                )));
            }

            let resolved = value.resolve(merged).map_err(ConfigError::Validation)?;

            let final_value = if var_type == VarType::Shell {
                resolve_shell_var(shell, &resolved)?
            } else {
                resolved
            };

            merged.insert(name.clone(), final_value.clone());
            project.vars.insert(name, final_value);
        }
        Stmt::FnDecl { name, body, .. } => {
            if project.functions.contains_key(&name) {
                return Err(ConfigError::Validation(format!(
                    "duplicate function in project '{}': {}",
                    project.name, name
                )));
            }
            project.functions.insert(name, body);
        }
        Stmt::RunDecl { name, chains, .. } => {
            if project.runs.contains_key(&name) {
                return Err(ConfigError::Validation(format!(
                    "duplicate run block in project '{}': {}",
                    project.name, name
                )));
            }
            project.runs.insert(name, chains);
        }
        _ => {}
    }
    Ok(())
}
