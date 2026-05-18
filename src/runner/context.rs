use crate::config::{Config, ConfigError, Project};
use crate::dsl::ast::{Expr, FnStmt};
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
            }
        }
        Ok(())
    }

    fn exec_log(&mut self, value: &Expr) -> Result<(), ConfigError> {
        let msg = self.resolve_expr(value)?;
        let indent = "  ".repeat(self.env_stack.len());
        let line = format!("{}log  {}", indent, msg);
        if let Some(callback) = self.output_callback {
            callback(line);
        } else {
            writeln!(self.writer, "\x1b[38;2;255;203;107m{}\x1b[0m", line)
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
            writeln!(self.writer, "\x1b[38;2;91;156;246m{}\x1b[0m", line)
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

        let output_cb = self.output_callback.cloned();
        let stdout_indent = "  ".repeat(self.env_stack.len() + 1);

        let stdout_thread = child.stdout.take().map(|stdout| {
            let indent = stdout_indent.clone();
            let cb = output_cb.clone();
            thread::spawn(move || -> Result<(), ConfigError> {
                let reader = std::io::BufReader::new(stdout);
                for line in reader.lines() {
                    let line =
                        line.map_err(|e| ConfigError::Validation(format!("read error: {}", e)))?;
                    let formatted = format!("{}{}", indent, line);
                    if let Some(ref callback) = cb {
                        callback(formatted);
                    }
                }
                Ok(())
            })
        });

        let stderr_thread = child.stderr.take().map(|stderr| {
            let indent = stdout_indent.clone();
            let cb = output_cb.clone();
            thread::spawn(move || -> Result<(), ConfigError> {
                let reader = std::io::BufReader::new(stderr);
                for line in reader.lines() {
                    let line =
                        line.map_err(|e| ConfigError::Validation(format!("read error: {}", e)))?;
                    let formatted = format!("{}{}", indent, line);
                    if let Some(ref callback) = cb {
                        callback(formatted);
                    }
                }
                Ok(())
            })
        });

        if let Some(handle) = stdout_thread {
            handle
                .join()
                .map_err(|_| ConfigError::Validation("stdout reader panicked".to_string()))??;
        }

        if let Some(handle) = stderr_thread {
            handle
                .join()
                .map_err(|_| ConfigError::Validation("stderr reader panicked".to_string()))??;
        }

        let status = child
            .wait()
            .map_err(|e| ConfigError::Validation(format!("exec failed: {}: {}", cmd_str, e)))?;

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
            writeln!(self.writer, "\x1b[38;2;255;203;107m{}\x1b[0m", line)
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
            writeln!(self.writer, "\x1b[38;2;199;146;234m{}\x1b[0m", line)
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
