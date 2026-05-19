use crate::colors;
use crate::config::{Config, ConfigError, Project};
use crate::dsl::ast::{CasePattern, Expr, FnStmt};
use std::collections::HashMap;
use std::io::{BufRead, Write};
use std::path::PathBuf;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::thread;

pub type OutputCallback = Arc<dyn Fn(String) + Send + Sync>;

pub struct ExecContext<'a> {
    pub(super) cfg: &'a Config,
    pub(super) project: &'a Project,
    pub(super) writer: &'a mut dyn Write,
    pub(super) output_callback: Option<&'a OutputCallback>,
    pub(super) vars: HashMap<String, String>,
    pub(super) env_stack: Vec<HashMap<String, String>>,
    pub(super) work_dir: PathBuf,
}

impl<'a> ExecContext<'a> {
    pub(super) fn new(
        cfg: &'a Config,
        project: &'a Project,
        writer: &'a mut dyn Write,
        output_callback: Option<&'a OutputCallback>,
    ) -> Self {
        ExecContext {
            cfg,
            project,
            writer,
            output_callback,
            vars: project.vars.clone(),
            env_stack: Vec::new(),
            work_dir: PathBuf::from(&cfg.sanctuary).join(&project.dir),
        }
    }

    pub(super) fn exec_fn_body(&mut self, body: &[FnStmt]) -> Result<(), ConfigError> {
        for stmt in body {
            match stmt {
                FnStmt::Log { value, .. } => self.exec_log(value)?,
                FnStmt::Exec { value, .. } => self.exec_exec(value)?,
                FnStmt::Cd { arg, .. } => self.exec_cd(arg)?,
                FnStmt::VarDecl {
                    name,
                    value,
                    var_type,
                    ..
                } => self.exec_var_decl(name, value, var_type)?,
                FnStmt::EnvBlock {
                    pairs,
                    body: block_body,
                    ..
                } => self.exec_env_block(pairs, block_body)?,
                FnStmt::Case { condition, arms } => {
                    let value = self.resolve_expr(condition)?;
                    for arm in arms {
                        if self.match_case_pattern(&arm.pattern, &value)? {
                            self.exec_fn_body(&arm.body)?;
                            break;
                        }
                    }
                }
            }
        }
        Ok(())
    }

    fn match_case_pattern(
        &mut self,
        pattern: &CasePattern,
        value: &str,
    ) -> Result<bool, ConfigError> {
        match pattern {
            CasePattern::Default => Ok(true),
            CasePattern::Literal { parts } => {
                let mut resolved = String::new();
                for part in parts {
                    if part.is_var {
                        match self.vars.get(&part.value) {
                            Some(v) => resolved.push_str(v),
                            None => {
                                return Err(ConfigError::Validation(format!(
                                    "undefined variable: ${}",
                                    part.value
                                )));
                            }
                        }
                    } else {
                        resolved.push_str(&part.value);
                    }
                }
                Ok(value == resolved)
            }
            CasePattern::VarRef { name } => match self.vars.get(name) {
                Some(v) => Ok(value == v),
                None => Err(ConfigError::Validation(format!(
                    "undefined variable: ${}",
                    name
                ))),
            },
        }
    }

    fn exec_log(&mut self, value: &Expr) -> Result<(), ConfigError> {
        let msg = self.resolve_expr(value)?;
        let indent = "  ".repeat(self.env_stack.len());
        let line = format!("{}log  {}", indent, msg);
        if let Some(callback) = self.output_callback {
            callback(line);
        } else {
            writeln!(self.writer, "{}{}{}", colors::LOG_ANSI, line, colors::RESET)
                .map_err(|e| ConfigError::Validation(format!("write error: {}", e)))?;
        }
        Ok(())
    }

    fn exec_exec(&mut self, value: &Expr) -> Result<(), ConfigError> {
        let cmd_str = self.resolve_expr(value)?;
        let indent = "  ".repeat(self.env_stack.len());
        let line = format!("{}exec {}", indent, cmd_str);

        if let Some(callback) = self.output_callback {
            callback(line);
        } else {
            writeln!(
                self.writer,
                "{}{}{}",
                colors::EXEC_ANSI,
                line,
                colors::RESET
            )
            .map_err(|e| ConfigError::Validation(format!("write error: {}", e)))?;
        }

        let mut child = Command::new(&self.cfg.shell)
            .arg("-c")
            .arg(&cmd_str)
            .current_dir(&self.work_dir)
            .envs(self.build_env())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| ConfigError::Validation(format!("exec failed: {}: {}", cmd_str, e)))?;

        let stdout_indent = "  ".repeat(self.env_stack.len() + 1);

        let status = if let Some(cb) = self.output_callback {
            let cb: OutputCallback = cb.clone();
            let stdout_thread = child.stdout.take().map(|stdout| {
                let indent = stdout_indent.clone();
                let cb = cb.clone();
                thread::spawn(move || -> Result<(), ConfigError> {
                    let reader = std::io::BufReader::new(stdout);
                    for line in reader.lines() {
                        let line = line
                            .map_err(|e| ConfigError::Validation(format!("read error: {}", e)))?;
                        cb(format!("{}{}", indent, line));
                    }
                    Ok(())
                })
            });

            let stderr_thread = child.stderr.take().map(|stderr| {
                let indent = stdout_indent.clone();
                let cb = cb.clone();
                thread::spawn(move || -> Result<(), ConfigError> {
                    let reader = std::io::BufReader::new(stderr);
                    for line in reader.lines() {
                        let line = line
                            .map_err(|e| ConfigError::Validation(format!("read error: {}", e)))?;
                        cb(format!("{}{}", indent, line));
                    }
                    Ok(())
                })
            });

            let stdout_result = stdout_thread.map(|handle| {
                handle
                    .join()
                    .map_err(|_| ConfigError::Validation("stdout reader panicked".to_string()))
            });
            let stderr_result = stderr_thread.map(|handle| {
                handle
                    .join()
                    .map_err(|_| ConfigError::Validation("stderr reader panicked".to_string()))
            });

            let status = child
                .wait()
                .map_err(|e| ConfigError::Validation(format!("exec failed: {}: {}", cmd_str, e)))?;

            if let Some(result) = stdout_result {
                result??;
            }
            if let Some(result) = stderr_result {
                result??;
            }

            status
        } else {
            let output = child
                .wait_with_output()
                .map_err(|e| ConfigError::Validation(format!("exec failed: {}: {}", cmd_str, e)))?;
            for line in std::io::BufReader::new(&output.stdout[..]).lines() {
                let line =
                    line.map_err(|e| ConfigError::Validation(format!("read error: {}", e)))?;
                writeln!(self.writer, "{}{}", stdout_indent, line)
                    .map_err(|e| ConfigError::Validation(format!("write error: {}", e)))?;
            }
            for line in std::io::BufReader::new(&output.stderr[..]).lines() {
                let line =
                    line.map_err(|e| ConfigError::Validation(format!("read error: {}", e)))?;
                writeln!(self.writer, "{}{}", stdout_indent, line)
                    .map_err(|e| ConfigError::Validation(format!("write error: {}", e)))?;
            }
            output.status
        };

        if !status.success() {
            return Err(ConfigError::Validation(format!(
                "exec failed with exit code: {}",
                status.code().unwrap_or(-1)
            )));
        }

        Ok(())
    }

    fn exec_cd(&mut self, arg: &str) -> Result<(), ConfigError> {
        let base_dir = PathBuf::from(&self.cfg.sanctuary).join(&self.project.dir);
        self.work_dir = if arg == "." {
            base_dir
        } else {
            base_dir.join(arg)
        };

        if !self.work_dir.exists() {
            return Err(ConfigError::Validation(format!(
                "cd {}: directory does not exist",
                arg
            )));
        }

        let indent = "  ".repeat(self.env_stack.len());
        let line = format!("{}cd   {}", indent, arg);
        if let Some(callback) = self.output_callback {
            callback(line);
        } else {
            writeln!(self.writer, "{}{}{}", colors::CD_ANSI, line, colors::RESET)
                .map_err(|e| ConfigError::Validation(format!("write error: {}", e)))?;
        }
        Ok(())
    }

    fn exec_var_decl(
        &mut self,
        name: &str,
        value: &Expr,
        var_type: &crate::dsl::ast::VarType,
    ) -> Result<(), ConfigError> {
        let val = self.resolve_expr(value)?;

        if var_type == &crate::dsl::ast::VarType::Shell {
            let output = Command::new(&self.cfg.shell)
                .arg("-c")
                .arg(&val)
                .current_dir(&self.work_dir)
                .envs(self.build_env())
                .output()
                .map_err(|e| {
                    ConfigError::Validation(format!(
                        "shell execution failed for var {}: {}",
                        name, e
                    ))
                })?;

            if !output.status.success() {
                return Err(ConfigError::Validation(format!(
                    "shell execution failed for var {}",
                    name
                )));
            }

            let result = String::from_utf8_lossy(&output.stdout)
                .trim_end()
                .to_string();
            self.vars.insert(name.to_string(), result);
        } else {
            self.vars.insert(name.to_string(), val);
        }
        Ok(())
    }

    fn exec_env_block(
        &mut self,
        pairs: &[crate::dsl::ast::EnvPair],
        body: &[FnStmt],
    ) -> Result<(), ConfigError> {
        let mut layer = HashMap::new();
        for pair in pairs {
            let val = self.resolve_expr(&pair.value)?;
            layer.insert(pair.key.clone(), val);
        }

        let keys: Vec<&str> = pairs.iter().map(|p| p.key.as_str()).collect();
        let indent = "  ".repeat(self.env_stack.len());
        let line = format!("{}env  {}", indent, keys.join(", "));

        if let Some(callback) = self.output_callback {
            callback(line);
        } else {
            writeln!(self.writer, "{}{}{}", colors::ENV_ANSI, line, colors::RESET)
                .map_err(|e| ConfigError::Validation(format!("write error: {}", e)))?;
        }

        self.env_stack.push(layer);

        let saved_vars = self.vars.clone();
        let result = self.exec_fn_body(body);

        self.vars = saved_vars;
        self.env_stack.pop();

        result
    }
}
