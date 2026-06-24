use crate::config::error::{ConfigError, SpannedValidationError};
use crate::config::types::{Config, Project};
use crate::dsl::ast::{Program, Stmt};
use crate::dsl::{Expr, FnStmt, VarType};
use crate::runner;
use std::collections::HashMap;
use std::path::Path;

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

/// Resolve an expression against string vars and shell vars.
/// Shell vars are executed on first reference and cached in `vars`.
fn resolve_expr_merged(
    expr: &Expr,
    vars: &mut HashMap<String, String>,
    shell_vars: &mut HashMap<String, String>,
    source_name: &str,
    source_text: &str,
) -> Result<String, ConfigError> {
    let err_for =
        |msg: String, o: usize, l: usize| spanned_err(msg, source_name, source_text, o, l);
    match expr {
        Expr::VarRef { name, offset, len } => {
            if let Some(val) = vars.get(name) {
                return Ok(val.clone());
            }
            if let Some(cmd) = shell_vars.remove(name) {
                resolve_shell_and_cache(name, &cmd, vars)?;
                return Ok(vars[name].clone());
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
                    if let Some(val) = vars.get(&part.value) {
                        result.push_str(val);
                    } else if let Some(cmd) = shell_vars.remove(&part.value) {
                        resolve_shell_and_cache(&part.value, &cmd, vars)?;
                        result.push_str(&vars[&part.value]);
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

fn resolve_shell_and_cache(
    name: &str,
    cmd: &str,
    vars: &mut HashMap<String, String>,
) -> Result<(), ConfigError> {
    let out =
        runner::exec_and_get_stdout(cmd, None::<&Path>, None::<&HashMap<String, String>>, None)
            .map_err(|e| ConfigError::Validation(format!("shell var ${} failed: {}", name, e)))?;
    vars.insert(name.to_string(), out.stdout);
    Ok(())
}

pub(crate) fn merge(programs: Vec<Program>) -> Result<Config, ConfigError> {
    let (mut global_vars, mut global_shell_vars) = collect_global_vars(&programs)?;
    let (sanctuary_expr, projects, config_fns, config_runs) =
        collect_projects(programs, &mut global_vars, &mut global_shell_vars)?;

    let sanctuary = match sanctuary_expr {
        Some(ref expr) => {
            resolve_expr_merged(expr, &mut global_vars, &mut global_shell_vars, "", "")?
        }
        None => String::new(),
    };

    Ok(Config {
        sanctuary,
        projects,
        vars: global_vars,
        shell_vars: global_shell_vars,
        functions: config_fns,
        runs: config_runs,
    })
}

fn collect_global_vars(
    programs: &[Program],
) -> Result<(HashMap<String, String>, HashMap<String, String>), ConfigError> {
    let mut vars = HashMap::new();
    let mut shell_vars = HashMap::new();
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
                if vars.contains_key(name) || shell_vars.contains_key(name) {
                    return Err(spanned_err(
                        format!("duplicate variable: {}", name),
                        &program.source_name,
                        &program.source_text,
                        *offset,
                        *len,
                    ));
                }

                let resolved = resolve_expr_merged(
                    value,
                    &mut vars,
                    &mut shell_vars,
                    &program.source_name,
                    &program.source_text,
                )?;

                if var_type == &VarType::Shell {
                    shell_vars.insert(name.clone(), resolved);
                } else {
                    vars.insert(name.clone(), resolved);
                }
            }
        }
    }
    Ok((vars, shell_vars))
}

#[allow(clippy::type_complexity)]
fn collect_projects(
    programs: Vec<Program>,
    global_vars: &mut HashMap<String, String>,
    global_shell_vars: &mut HashMap<String, String>,
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
                        shell_vars: HashMap::new(),
                        functions: HashMap::new(),
                        runs: HashMap::new(),
                    };

                    let mut seen_fields = std::collections::HashSet::new();
                    for field in &fields {
                        if !seen_fields.insert(field.key.as_str()) {
                            return Err(spanned_err(
                                format!("duplicate field '{}' in project '{}'", field.key, name),
                                &program.source_name,
                                &program.source_text,
                                field.value.span().0,
                                field.value.span().1,
                            ));
                        }
                        let value = resolve_expr_merged(
                            &field.value,
                            global_vars,
                            global_shell_vars,
                            &program.source_name,
                            &program.source_text,
                        )?;
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
                    let mut merged_shell_vars = global_shell_vars.clone();
                    for body_stmt in body {
                        merge_project_body_stmt(
                            &mut project,
                            body_stmt,
                            &mut merged_vars,
                            &mut merged_shell_vars,
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
    merged_shell_vars: &mut HashMap<String, String>,
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
            if project.vars.contains_key(&name) || project.shell_vars.contains_key(&name) {
                return Err(make_err(
                    format!("duplicate variable in project '{}': {}", project.name, name),
                    offset,
                    len,
                ));
            }

            let resolved =
                resolve_expr_merged(&value, merged, merged_shell_vars, source_name, source_text)?;

            if var_type == VarType::Shell {
                merged_shell_vars.insert(name.clone(), resolved.clone());
                project.shell_vars.insert(name.clone(), resolved);
            } else {
                merged.insert(name.clone(), resolved.clone());
                project.vars.insert(name, resolved);
            }
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
