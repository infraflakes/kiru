use crate::compiler::error::{CompileError, spanned_err};
use crate::compiler::resolve::{exec_shell_var, resolve_expr_merged};
use crate::compiler::types::{Project, Sanctuary, SyncMode};
use crate::dsl::{Expr, FnStmt, Program, ProjectField, Stmt, VarType};
use std::collections::HashMap;
use std::collections::HashSet;

type ProjectsResult = Result<
    (
        Option<Expr>,
        HashMap<String, Project>,
        HashMap<String, Vec<FnStmt>>,
        HashMap<String, Vec<Vec<String>>>,
    ),
    CompileError,
>;

fn parse_sync_mode(
    value: &str,
    source_name: &str,
    source_text: &str,
    offset: usize,
    len: usize,
) -> Result<SyncMode, CompileError> {
    match value {
        "clone" => Ok(SyncMode::Clone),
        "ignore" => Ok(SyncMode::Ignore),
        _ => Err(spanned_err(
            format!(
                "invalid sync value {:?} (expected 'clone' or 'ignore')",
                value
            ),
            source_name,
            source_text,
            offset,
            len,
        )),
    }
}

pub(crate) fn merge(programs: Vec<Program>) -> Result<Sanctuary, CompileError> {
    let mut global_vars = collect_global_vars(&programs)?;
    let (sanctuary_expr, projects, config_fns, config_runs) =
        collect_projects(programs, &mut global_vars)?;

    let sanctuary_path = match sanctuary_expr {
        Some(ref expr) => {
            let empty = HashMap::new();
            resolve_expr_merged(expr, &global_vars, &empty, "", "")?
        }
        None => String::new(),
    };

    Ok(Sanctuary {
        sanctuary_path,
        projects,
        vars: global_vars,
        functions: config_fns,
        runs: config_runs,
    })
}

fn collect_global_vars(programs: &[Program]) -> Result<HashMap<String, String>, CompileError> {
    let mut vars = HashMap::new();
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
                let empty = HashMap::new();
                let resolved = resolve_expr_merged(
                    value,
                    &vars,
                    &empty,
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
                    vars.insert(name.clone(), output);
                } else {
                    vars.insert(name.clone(), resolved);
                }
            }
        }
    }
    Ok(vars)
}

fn collect_projects(
    programs: Vec<Program>,
    global_vars: &mut HashMap<String, String>,
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
                        return Err(spanned_err(
                            "duplicate sanctuary declaration".to_string(),
                            &program.source_name,
                            &program.source_text,
                            0,
                            1,
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
                        sync: SyncMode::Clone,
                        include_file: None,
                        branch: None,
                        vars: HashMap::new(),
                        functions: HashMap::new(),
                        runs: HashMap::new(),
                    };

                    let mut seen_fields: HashSet<String> = HashSet::new();

                    for body_stmt in body {
                        merge_project_body_stmt(
                            &mut project,
                            body_stmt,
                            global_vars,
                            &program.source_name,
                            &program.source_text,
                            &mut seen_fields,
                        )?;
                    }

                    projects.insert(name, project);
                }
                Stmt::Field {
                    key, offset, len, ..
                } => {
                    return Err(spanned_err(
                        format!(
                            "unexpected field '{:?}' at top level (fields are only valid inside a project block)",
                            key
                        ),
                        &program.source_name,
                        &program.source_text,
                        offset,
                        len,
                    ));
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
                Stmt::Var { .. } => {}
            }
        }
    }

    Ok((sanctuary_expr, projects, config_fns, config_runs))
}

pub(crate) fn merge_project_body_stmt(
    project: &mut Project,
    stmt: Stmt,
    global_vars: &mut HashMap<String, String>,
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
                resolve_expr_merged(&value, global_vars, &project.vars, source_name, source_text)?;

            if var_type == VarType::Shell {
                let output =
                    exec_shell_var(&name, &resolved, source_name, source_text, offset, len)?;
                project.vars.insert(name, output);
            } else {
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
            let field_name = format!("{:?}", key);
            if !seen_fields.insert(field_name) {
                return Err(make_err(
                    format!("duplicate field '{:?}' in project '{}'", key, project.name),
                    offset,
                    len,
                ));
            }

            let resolved =
                resolve_expr_merged(&value, global_vars, &project.vars, source_name, source_text)?;
            match key {
                ProjectField::Url => project.url = resolved,
                ProjectField::Dir => project.dir = resolved,
                ProjectField::Sync => {
                    project.sync =
                        parse_sync_mode(&resolved, source_name, source_text, offset, len)?;
                }
                ProjectField::Include => {
                    if !resolved.is_empty() {
                        project.include_file = Some(resolved);
                    }
                }
                ProjectField::Branch => {
                    if resolved.is_empty() {
                        project.branch = None;
                    } else {
                        project.branch = Some(resolved);
                    }
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
        Stmt::Sanctuary { .. } | Stmt::Project { .. } => {
            return Err(spanned_err(
                format!(
                    "unexpected statement in project '{}' (only var, fn, run, and fields are valid)",
                    project.name
                ),
                source_name,
                source_text,
                0,
                1,
            ));
        }
    }
    Ok(())
}
