use crate::config::error::ConfigError;
use crate::config::types::{Config, Project};
use crate::dsl::ast::{Expr, Program, Stmt, VarType};
use std::collections::HashMap;
use std::io::Read;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

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
                    if shell.is_empty() {
                        return Err(ConfigError::Validation(
                            "shell must be declared before using shell variables".to_string(),
                        ));
                    }
                    execute_shell(shell, &resolved)?
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
                        use_file: None,
                        branch: String::new(),
                        vars: HashMap::new(),
                        functions: HashMap::new(),
                        seqs: HashMap::new(),
                        pars: HashMap::new(),
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
                            "use" => {
                                if !value.is_empty() {
                                    project.use_file = Some(value);
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

                    for body_stmt in body {
                        merge_project_body_stmt(&mut project, body_stmt, shell)?;
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

            let resolved = value
                .resolve(&project.vars)
                .map_err(ConfigError::Validation)?;

            let final_value = if var_type == VarType::Shell {
                if shell.is_empty() {
                    return Err(ConfigError::Validation(
                        "shell must be declared before using shell variables".to_string(),
                    ));
                }
                execute_shell(shell, &resolved)?
            } else {
                resolved
            };

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
        Stmt::SeqDecl { name, fns, .. } => {
            if project.seqs.contains_key(&name) {
                return Err(ConfigError::Validation(format!(
                    "duplicate seq in project '{}': {}",
                    project.name, name
                )));
            }
            project.seqs.insert(name, fns);
        }
        Stmt::ParDecl { name, fns, .. } => {
            if project.pars.contains_key(&name) {
                return Err(ConfigError::Validation(format!(
                    "duplicate par in project '{}': {}",
                    project.name, name
                )));
            }
            project.pars.insert(name, fns);
        }
        _ => {}
    }
    Ok(())
}

const SHELL_TIMEOUT: Duration = Duration::from_secs(30);
const POLL_INTERVAL: Duration = Duration::from_millis(50);

fn collect_output(child: &mut std::process::Child) -> (Vec<u8>, Vec<u8>) {
    let mut stdout_buf = Vec::new();
    let mut stderr_buf = Vec::new();
    let _ = child
        .stdout
        .as_mut()
        .map(|s| s.read_to_end(&mut stdout_buf));
    let _ = child
        .stderr
        .as_mut()
        .map(|s| s.read_to_end(&mut stderr_buf));
    (stdout_buf, stderr_buf)
}

pub(crate) fn execute_shell(shell: &str, command: &str) -> Result<String, ConfigError> {
    let mut child = Command::new(shell)
        .arg("-c")
        .arg(command)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| ConfigError::Validation(format!("shell execution failed: {}", e)))?;

    let start = Instant::now();
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {
                if start.elapsed() >= SHELL_TIMEOUT {
                    child.kill().ok();
                    child.wait().ok();
                    let (stdout_buf, stderr_buf) = collect_output(&mut child);
                    let stderr = String::from_utf8_lossy(&stderr_buf);
                    let stdout = String::from_utf8_lossy(&stdout_buf);
                    let detail = if stderr.trim().is_empty() {
                        stdout.trim().to_string()
                    } else {
                        stderr.trim().to_string()
                    };
                    return if detail.is_empty() {
                        Err(ConfigError::Validation(format!(
                            "shell command timed out after {}s",
                            SHELL_TIMEOUT.as_secs()
                        )))
                    } else {
                        Err(ConfigError::Validation(format!(
                            "shell command timed out after {}s: {}",
                            SHELL_TIMEOUT.as_secs(),
                            detail
                        )))
                    };
                }
                std::thread::sleep(POLL_INTERVAL);
            }
            Err(e) => {
                return Err(ConfigError::Validation(format!(
                    "shell execution failed: {}",
                    e
                )));
            }
        }
    };

    let (stdout_buf, stderr_buf) = collect_output(&mut child);

    if !status.success() {
        let stderr = String::from_utf8_lossy(&stderr_buf);
        return Err(ConfigError::Validation(format!(
            "shell command failed: {}",
            stderr.trim()
        )));
    }

    let stdout = String::from_utf8_lossy(&stdout_buf);
    Ok(stdout.trim_end().to_string())
}
