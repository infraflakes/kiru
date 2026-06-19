use crate::config::error::{ConfigError, SpannedValidationError};
use crate::config::types::{Config, Project};
use crate::dsl::ast::{Expr, FnStmt, Program, Stmt, VarType};
use std::collections::HashMap;

fn spanned_err(
    msg: String,
    source_name: &str,
    source_text: &str,
    offset: usize,
    len: usize,
) -> ConfigError {
    ConfigError::ValidationReport(miette::Report::new(SpannedValidationError {
        message: msg,
        span: miette::SourceSpan::new(offset.into(), len.max(1)),
        source_code: miette::NamedSource::new(source_name, source_text.to_string()),
    }))
}

pub(crate) fn merge(programs: Vec<Program>) -> Result<Config, ConfigError> {
    let global_vars = collect_global_vars(&programs)?;
    let (sanctuary_expr, projects, config_fns, config_runs) =
        collect_projects(programs, &global_vars)?;

    let sanctuary = match sanctuary_expr {
        Some(ref expr) => expr
            .resolve(&global_vars)
            .map_err(ConfigError::Validation)?,
        None => String::new(),
    };

    Ok(Config {
        sanctuary,
        projects,
        vars: global_vars,
        functions: config_fns,
        runs: config_runs,
    })
}

fn resolve_shell_var(resolved: &str) -> Result<String, ConfigError> {
    let out = crate::shell::run_captured(
        resolved,
        None,
        None,
        Some(std::time::Duration::from_secs(30)),
    )
    .map_err(|e| ConfigError::Validation(e.to_string()))?;
    Ok(out.stdout)
}

fn collect_global_vars(programs: &[Program]) -> Result<HashMap<String, String>, ConfigError> {
    let mut global_vars = HashMap::new();
    for program in programs {
        for stmt in &program.stmts {
            if let Stmt::VarDecl {
                name,
                value,
                var_type,
                offset,
                len,
                ..
            } = stmt
            {
                if global_vars.contains_key(name) {
                    return Err(spanned_err(
                        format!("duplicate variable: {}", name),
                        &program.source_name,
                        &program.source_text,
                        *offset,
                        *len,
                    ));
                }

                let resolved = value.resolve(&global_vars).map_err(|e| {
                    let (o, l) = value.span();
                    spanned_err(e, &program.source_name, &program.source_text, o, l)
                })?;

                let final_value = if var_type == &VarType::Shell {
                    resolve_shell_var(&resolved)?
                } else {
                    resolved
                };

                global_vars.insert(name.clone(), final_value);
            }
        }
    }
    Ok(global_vars)
}

#[allow(clippy::type_complexity)]
fn collect_projects(
    programs: Vec<Program>,
    global_vars: &HashMap<String, String>,
) -> Result<
    (
        Option<Expr>,
        HashMap<String, Project>,
        HashMap<String, Vec<FnStmt>>,
        HashMap<String, Vec<Vec<String>>>,
    ),
    ConfigError,
> {
    let mut sanctuary_expr: Option<Expr> = None;
    let mut projects: HashMap<String, Project> = HashMap::new();
    let mut config_fns: HashMap<String, Vec<FnStmt>> = HashMap::new();
    let mut config_runs: HashMap<String, Vec<Vec<String>>> = HashMap::new();

    for program in programs {
        for stmt in program.stmts {
            match stmt {
                Stmt::SanctuaryDecl { value } => {
                    if sanctuary_expr.is_some() {
                        return Err(ConfigError::Validation(
                            "duplicate sanctuary declaration".to_string(),
                        ));
                    }
                    sanctuary_expr = Some(value);
                }
                Stmt::ProjectDecl {
                    name,
                    fields,
                    body,
                    offset,
                    len,
                    ..
                } => {
                    if projects.contains_key(&name) {
                        return Err(spanned_err(
                            format!("duplicate project: {}", name),
                            &program.source_name,
                            &program.source_text,
                            offset,
                            len,
                        ));
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

                    let mut seen_fields = std::collections::HashSet::new();
                    for field in &fields {
                        if !seen_fields.insert(field.key.as_str()) {
                            return Err(spanned_err(
                                format!(
                                    "duplicate field '{}' in project '{}'",
                                    field.key, name
                                ),
                                &program.source_name,
                                &program.source_text,
                                field.value.span().0,
                                field.value.span().1,
                            ));
                        }
                        let value = field.value.resolve(global_vars).map_err(|e| {
                            let (o, l) = field.value.span();
                            spanned_err(e, &program.source_name, &program.source_text, o, l)
                        })?;
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
                        merge_project_body_stmt(
                            &mut project,
                            body_stmt,
                            &mut merged_vars,
                            &program.source_name,
                            &program.source_text,
                        )?;
                    }

                    projects.insert(name, project);
                }
                Stmt::FnDecl {
                    name,
                    body,
                    offset,
                    len,
                    ..
                } => {
                    if config_fns.contains_key(&name) {
                        return Err(spanned_err(
                            format!("duplicate top-level function: {}", name),
                            &program.source_name,
                            &program.source_text,
                            offset,
                            len,
                        ));
                    }
                    config_fns.insert(name, body);
                }
                Stmt::RunDecl {
                    name,
                    chains,
                    offset,
                    len,
                    ..
                } => {
                    if config_runs.contains_key(&name) {
                        return Err(spanned_err(
                            format!("duplicate top-level run block: {}", name),
                            &program.source_name,
                            &program.source_text,
                            offset,
                            len,
                        ));
                    }
                    config_runs.insert(name, chains);
                }
                _ => {}
            }
        }
    }

    Ok((sanctuary_expr, projects, config_fns, config_runs))
}

pub(crate) fn merge_project_body_stmt(
    project: &mut Project,
    stmt: Stmt,
    merged: &mut HashMap<String, String>,
    source_name: &str,
    source_text: &str,
) -> Result<(), ConfigError> {
    let make_err = |msg: String, offset: usize, len: usize| -> ConfigError {
        spanned_err(msg, source_name, source_text, offset, len)
    };
    match stmt {
        Stmt::VarDecl {
            name,
            value,
            var_type,
            offset,
            len,
            ..
        } => {
            if project.vars.contains_key(&name) {
                return Err(make_err(
                    format!("duplicate variable in project '{}': {}", project.name, name),
                    offset,
                    len,
                ));
            }

            let resolved = value.resolve(merged).map_err(|e| {
                let (o, l) = value.span();
                spanned_err(e, source_name, source_text, o, l)
            })?;

            let final_value = if var_type == VarType::Shell {
                resolve_shell_var(&resolved)?
            } else {
                resolved
            };

            merged.insert(name.clone(), final_value.clone());
            project.vars.insert(name, final_value);
        }
        Stmt::FnDecl {
            name,
            body,
            offset,
            len,
            ..
        } => {
            if project.functions.contains_key(&name) {
                return Err(make_err(
                    format!("duplicate function in project '{}': {}", project.name, name),
                    offset,
                    len,
                ));
            }
            project.functions.insert(name, body);
        }
        Stmt::RunDecl {
            name,
            chains,
            offset,
            len,
            ..
        } => {
            if project.runs.contains_key(&name) {
                return Err(make_err(
                    format!(
                        "duplicate run block in project '{}': {}",
                        project.name, name
                    ),
                    offset,
                    len,
                ));
            }
            project.runs.insert(name, chains);
        }
        _ => {}
    }
    Ok(())
}
