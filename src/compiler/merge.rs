use crate::compiler::error::{CompileError, SpannedValidationError};
use crate::compiler::types::{Project, Sanctuary};
use crate::dsl::ast::{Program, Stmt};
use crate::dsl::{Expr, FnStmt, VarType};
use crate::runner;
use std::collections::HashMap;
use std::collections::HashSet;

type VarsResult = Result<(HashMap<String, String>, HashMap<String, String>), CompileError>;
type ProjectsResult = Result<
    (
        Option<Expr>,
        HashMap<String, Project>,
        HashMap<String, Vec<FnStmt>>,
        HashMap<String, Vec<Vec<String>>>,
    ),
    CompileError,
>;

fn spanned_err(
    msg: String,
    source_name: &str,
    source_text: &str,
    offset: usize,
    len: usize,
) -> CompileError {
    CompileError::ValidationReport(miette::Report::new(SpannedValidationError {
        message: msg,
        span: miette::SourceSpan::new(offset.into(), len.max(1)),
        source_code: miette::NamedSource::new(source_name, source_text.to_string()),
    }))
}

fn resolve_expr_merged(
    expr: &Expr,
    vars: &mut HashMap<String, String>,
    shell_vars: &mut HashMap<String, String>,
    source_name: &str,
    source_text: &str,
) -> Result<String, CompileError> {
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
) -> Result<(), CompileError> {
    let out = runner::exec_and_get_stdout(cmd, None, None)
        .map_err(|e| CompileError::Validation(format!("shell var ${} failed: {}", name, e)))?;
    vars.insert(name.to_string(), out.stdout);
    Ok(())
}

fn exec_shell_var(
    name: &str,
    resolved_command: &str,
    source_name: &str,
    source_text: &str,
    offset: usize,
    len: usize,
) -> Result<String, CompileError> {
    let out = runner::exec_and_get_stdout(resolved_command, None, None).map_err(|e| {
        spanned_err(
            format!("shell var ${} failed: {}", name, e),
            source_name,
            source_text,
            offset,
            len,
        )
    })?;
    Ok(out.stdout)
}

pub(crate) fn merge(programs: Vec<Program>) -> Result<Sanctuary, CompileError> {
    let (mut global_vars, mut global_shell_vars) = collect_global_vars(&programs)?;
    let (sanctuary_expr, projects, config_fns, config_runs) =
        collect_projects(programs, &mut global_vars, &mut global_shell_vars)?;

    let sanctuary_path = match sanctuary_expr {
        Some(ref expr) => {
            resolve_expr_merged(expr, &mut global_vars, &mut global_shell_vars, "", "")?
        }
        None => String::new(),
    };

    Ok(Sanctuary {
        sanctuary_path,
        projects,
        vars: global_vars,
        shell_vars: global_shell_vars,
        functions: config_fns,
        runs: config_runs,
    })
}

fn collect_global_vars(programs: &[Program]) -> VarsResult {
    let mut vars = HashMap::new();
    let mut shell_vars = HashMap::new();
    for program in programs {
        for stmt in &program.stmts {
            if let Stmt::Var {
                name,
                value,
                var_type,
                offset,
                len,
                ..
            } = stmt
            {
                let resolved = resolve_expr_merged(
                    value,
                    &mut vars,
                    &mut shell_vars,
                    &program.source_name,
                    &program.source_text,
                )?;

                if var_type == &VarType::Shell {
                    let output = exec_shell_var(
                        name,
                        &resolved,
                        &program.source_name,
                        &program.source_text,
                        *offset,
                        *len,
                    )?;
                    vars.remove(name);
                    vars.insert(name.clone(), output);
                    shell_vars.remove(name);
                } else {
                    shell_vars.remove(name);
                    vars.remove(name);
                    vars.insert(name.clone(), resolved);
                }
            }
        }
    }
    Ok((vars, shell_vars))
}

fn collect_projects(
    programs: Vec<Program>,
    global_vars: &mut HashMap<String, String>,
    global_shell_vars: &mut HashMap<String, String>,
) -> ProjectsResult {
    let mut sanctuary_expr: Option<Expr> = None;
    let mut projects: HashMap<String, Project> = HashMap::new();
    let mut config_fns: HashMap<String, Vec<FnStmt>> = HashMap::new();
    let mut config_runs: HashMap<String, Vec<Vec<String>>> = HashMap::new();

    for program in programs {
        for stmt in program.stmts {
            match stmt {
                Stmt::Sanctuary { value } => {
                    if sanctuary_expr.is_some() {
                        return Err(CompileError::Validation(
                            "duplicate sanctuary declaration".to_string(),
                        ));
                    }
                    sanctuary_expr = Some(value);
                }
                Stmt::Project {
                    name,
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

                    let mut merged_vars = global_vars.clone();
                    let mut merged_shell_vars = global_shell_vars.clone();
                    let mut seen_fields: HashSet<String> = HashSet::new();

                    for body_stmt in body {
                        merge_project_body_stmt(
                            &mut project,
                            body_stmt,
                            &mut merged_vars,
                            &mut merged_shell_vars,
                            &program.source_name,
                            &program.source_text,
                            &mut seen_fields,
                        )?;
                    }

                    projects.insert(name, project);
                }
                Stmt::Fn {
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
                Stmt::Run {
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
    seen_fields: &mut HashSet<String>,
) -> Result<(), CompileError> {
    let make_err = |msg: String, offset: usize, len: usize| -> CompileError {
        spanned_err(msg, source_name, source_text, offset, len)
    };
    match stmt {
        Stmt::Var {
            name,
            value,
            var_type,
            offset,
            len,
            ..
        } => {
            let resolved =
                resolve_expr_merged(&value, merged, merged_shell_vars, source_name, source_text)?;

            if var_type == VarType::Shell {
                let output =
                    exec_shell_var(&name, &resolved, source_name, source_text, offset, len)?;
                merged.remove(&name);
                merged.insert(name.clone(), output.clone());
                merged_shell_vars.remove(&name);
                project.vars.remove(&name);
                project.vars.insert(name, output);
            } else {
                merged_shell_vars.remove(&name);
                merged.remove(&name);
                merged.insert(name.clone(), resolved.clone());
                project.vars.remove(&name);
                project.vars.insert(name, resolved);
            }
        }
        Stmt::Field {
            key,
            value,
            offset,
            len,
            ..
        } => {
            if !seen_fields.insert(key.clone()) {
                return Err(make_err(
                    format!("duplicate field '{}' in project '{}'", key, project.name),
                    offset,
                    len,
                ));
            }

            let resolved =
                resolve_expr_merged(&value, merged, merged_shell_vars, source_name, source_text)?;
            match key.as_str() {
                "url" => project.url = resolved,
                "dir" => project.dir = resolved,
                "sync" => project.sync = resolved,
                "include" => {
                    if !resolved.is_empty() {
                        project.include_file = Some(resolved);
                    }
                }
                "branch" => project.branch = resolved,
                _ => {
                    return Err(CompileError::Validation(format!(
                        "unknown project field: {}",
                        key
                    )));
                }
            }
        }
        Stmt::Fn {
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
        Stmt::Run {
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
