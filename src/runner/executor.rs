use crate::colors;
use crate::config::{Config, Project};
use crate::ir::{CasePattern, Expr, FnStmt};
use crate::runner::Output;
use crate::runner::error::RuntimeError;
use crate::shell;
use std::collections::HashMap;
use std::io::BufRead;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;

pub type OutputCallback = Arc<dyn Fn(String) + Send + Sync>;

pub(crate) struct ExecContext<'a> {
    pub(super) cfg: &'a Config,
    pub(super) project: Option<&'a Project>,
    output: &'a mut Output,
    /// Base variables (global + project). Scope layers pushed/popped for case arms and env blocks.
    pub(super) vars: HashMap<String, String>,
    /// Scope stack for variable shadowing. Each layer shadows `vars` and higher layers.
    pub(super) var_stack: Vec<HashMap<String, String>>,
    pub(super) env_stack: Vec<HashMap<String, String>>,
    pub(super) work_dir: PathBuf,
    pub(super) sys_env: Vec<(String, String)>,
}

impl<'a> ExecContext<'a> {
    pub(crate) fn new(
        cfg: &'a Config,
        project: Option<&'a Project>,
        output: &'a mut Output,
    ) -> Self {
        let mut vars = cfg.vars.clone();
        if let Some(proj) = project {
            vars.extend(proj.vars.clone());
        }
        let work_dir = match project {
            Some(proj) => PathBuf::from(&cfg.sanctuary).join(&proj.dir),
            None => std::env::current_dir().unwrap_or_else(|_| PathBuf::from(".")),
        };
        ExecContext {
            cfg,
            project,
            output,
            vars,
            var_stack: Vec::new(),
            env_stack: Vec::new(),
            work_dir,
            sys_env: std::env::vars().collect(),
        }
    }

    fn current_shell() -> String {
        std::env::var("SHELL").unwrap_or_else(|_| "sh".to_string())
    }

    /// Resolve an expression, checking scope layers before base vars.
    pub(super) fn resolve_expr(&self, expr: &Expr) -> Result<String, RuntimeError> {
        match expr {
            Expr::VarRef { name, .. } => self
                .resolve_var(name)
                .cloned()
                .ok_or_else(|| RuntimeError::Lookup(format!("undefined variable: ${}", name))),
            Expr::BacktickLit { parts, .. } => {
                let mut result = String::new();
                for part in parts {
                    if part.is_var {
                        let val = self.resolve_var(&part.value).ok_or_else(|| {
                            RuntimeError::Lookup(format!("undefined variable: ${}", part.value))
                        })?;
                        result.push_str(val);
                    } else {
                        result.push_str(&part.value);
                    }
                }
                Ok(result)
            }
        }
    }

    /// Look up a variable name, checking scope layers (top-to-bottom) then base vars.
    fn resolve_var(&self, name: &str) -> Option<&String> {
        for layer in self.var_stack.iter().rev() {
            if let Some(val) = layer.get(name) {
                return Some(val);
            }
        }
        self.vars.get(name)
    }

    pub(super) fn build_env(&self) -> impl Iterator<Item = (String, String)> + '_ {
        let sys = self.sys_env.iter().map(|(k, v)| (k.clone(), v.clone()));
        let overrides = self
            .env_stack
            .iter()
            .flat_map(|layer| layer.iter().map(|(k, v)| (k.clone(), v.clone())));
        sys.chain(overrides)
    }

    fn indent(&self, extra: usize) -> String {
        "  ".repeat(self.env_stack.len() + extra)
    }

    pub(crate) fn exec_fn_body(&mut self, body: &[FnStmt]) -> Result<(), RuntimeError> {
        let saved_work_dir = self.work_dir.clone();
        self.var_stack.push(HashMap::new());
        let result = self.exec_fn_body_inner(body);
        self.var_stack.pop();
        self.work_dir = saved_work_dir;
        result
    }

    fn exec_fn_body_inner(&mut self, body: &[FnStmt]) -> Result<(), RuntimeError> {
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
                            self.var_stack.push(HashMap::new());
                            let result = self.exec_fn_body(&arm.body);
                            self.var_stack.pop();
                            result?;
                            break;
                        }
                    }
                }
            }
        }
        Ok(())
    }

    pub(super) fn match_case_pattern(
        &mut self,
        pattern: &CasePattern,
        value: &str,
    ) -> Result<bool, RuntimeError> {
        match pattern {
            CasePattern::Default => Ok(true),
            CasePattern::Literal { parts } => {
                let mut resolved = String::new();
                for part in parts {
                    if part.is_var {
                        match self.resolve_var(&part.value) {
                            Some(v) => resolved.push_str(v),
                            None => {
                                return Err(RuntimeError::Lookup(format!(
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
            CasePattern::VarRef { name } => match self.resolve_var(name) {
                Some(v) => Ok(value == v),
                None => Err(RuntimeError::Lookup(format!(
                    "undefined variable: ${}",
                    name
                ))),
            },
        }
    }

    fn exec_log(&mut self, value: &Expr) -> Result<(), RuntimeError> {
        let msg = self.resolve_expr(value)?;
        let indent = self.indent(0);
        let line = format!("{}log  {}", indent, msg);
        self.output
            .writeln_colored(&line, colors::LOG_ANSI)
            .map_err(RuntimeError::Io)?;
        Ok(())
    }

    fn exec_exec(&mut self, value: &Expr) -> Result<(), RuntimeError> {
        let cmd_str = self.resolve_expr(value)?;
        let indent = self.indent(0);
        let line = format!("{}exec {}", indent, cmd_str);
        self.output
            .writeln_colored(&line, colors::EXEC_ANSI)
            .map_err(RuntimeError::Io)?;

        let shell = Self::current_shell();
        let mut child = Command::new(&shell)
            .arg("-c")
            .arg(&cmd_str)
            .current_dir(&self.work_dir)
            .envs(self.build_env())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| RuntimeError::exec_io_error(&cmd_str, e))?;

        let indent = self.indent(1);

        let status = match self.output.fork_callback() {
            Some(cb) => {
                let stdout_thread =
                    spawn_stream_reader(child.stdout.take(), indent.clone(), cb.clone());
                let stderr_thread = spawn_stream_reader(child.stderr.take(), indent, cb);

                let status = child
                    .wait()
                    .map_err(|e| RuntimeError::exec_io_error(&cmd_str, e))?;

                if let Some(result) = stdout_thread.map(|h| h.join()) {
                    result
                        .map_err(|_| RuntimeError::Panic("stdout reader panicked".to_string()))??;
                }
                if let Some(result) = stderr_thread.map(|h| h.join()) {
                    result
                        .map_err(|_| RuntimeError::Panic("stderr reader panicked".to_string()))??;
                }

                status
            }
            None => {
                let output = child
                    .wait_with_output()
                    .map_err(|e| RuntimeError::exec_io_error(&cmd_str, e))?;
                write_output_lines(self.output, &output.stdout, &indent)?;
                write_output_lines(self.output, &output.stderr, &indent)?;
                output.status
            }
        };

        if !status.success() {
            return Err(RuntimeError::exec_exit_code(cmd_str, status.code()));
        }

        Ok(())
    }

    fn exec_cd(&mut self, arg: &Expr) -> Result<(), RuntimeError> {
        let resolved = self.resolve_expr(arg)?;

        if Path::new(&resolved).is_absolute() {
            return Err(RuntimeError::Lookup(format!(
                "cd {}: absolute path not allowed",
                resolved
            )));
        }

        let candidate = self.work_dir.join(&resolved);

        let candidate = std::fs::canonicalize(&candidate)
            .map_err(|e| RuntimeError::Lookup(format!("cd {}: {}", resolved, e)))?;

        if !candidate.is_dir() {
            return Err(RuntimeError::Lookup(format!(
                "cd {}: target is not a directory",
                resolved
            )));
        }

        if let Some(proj) = self.project {
            let base_canonical = PathBuf::from(&self.cfg.sanctuary).join(&proj.dir);
            let base_canonical = std::fs::canonicalize(&base_canonical)
                .map_err(|e| RuntimeError::Lookup(format!("cd {}: {}", resolved, e)))?;
            if !candidate.starts_with(&base_canonical) {
                return Err(RuntimeError::Lookup(format!(
                    "cd {}: path escapes project directory",
                    resolved
                )));
            }
        }

        self.work_dir = candidate;

        let indent = self.indent(0);
        let line = format!("{}cd   {}", indent, resolved);
        self.output
            .writeln_colored(&line, colors::CD_ANSI)
            .map_err(RuntimeError::Io)?;
        Ok(())
    }

    fn exec_var_decl(
        &mut self,
        name: &str,
        value: &Expr,
        var_type: &crate::ir::VarType,
    ) -> Result<(), RuntimeError> {
        let val = self.resolve_expr(value)?;

        let resolved = if var_type == &crate::ir::VarType::Shell {
            let env_map: HashMap<String, String> = self.build_env().collect();
            let out = shell::run_captured(&val, Some(&self.work_dir), Some(&env_map), None)
                .map_err(|e| match e {
                    shell::Error::Spawn(io_err) => RuntimeError::exec_io_error(&val, io_err),
                    shell::Error::Exit {
                        stderr, exit_code, ..
                    } => RuntimeError::Exec {
                        cmd: val.clone(),
                        exit_code,
                        detail: stderr,
                    },
                    shell::Error::Timeout { partial_stderr, .. } => RuntimeError::Exec {
                        cmd: val.clone(),
                        exit_code: None,
                        detail: format!("timed out: {}", partial_stderr),
                    },
                })?;
            out.stdout
        } else {
            val
        };

        if let Some(top) = self.var_stack.last_mut() {
            top.insert(name.to_string(), resolved);
        } else {
            self.vars.insert(name.to_string(), resolved);
        }
        Ok(())
    }

    fn exec_env_block(
        &mut self,
        pairs: &[crate::ir::EnvPair],
        body: &[FnStmt],
    ) -> Result<(), RuntimeError> {
        let mut layer = HashMap::new();
        for pair in pairs {
            let val = self.resolve_expr(&pair.value)?;
            layer.insert(pair.key.clone(), val);
        }

        let keys: Vec<&str> = pairs.iter().map(|p| p.key.as_str()).collect();
        let indent = self.indent(0);
        let line = format!("{}env  {}", indent, keys.join(", "));

        self.output
            .writeln_colored(&line, colors::ENV_ANSI)
            .map_err(RuntimeError::Io)?;

        self.env_stack.push(layer);
        self.var_stack.push(HashMap::new());
        let result = self.exec_fn_body(body);
        self.var_stack.pop();
        self.env_stack.pop();

        result
    }
}

fn spawn_stream_reader<R: std::io::Read + Send + 'static>(
    stream: Option<R>,
    indent: String,
    cb: OutputCallback,
) -> Option<std::thread::JoinHandle<Result<(), RuntimeError>>> {
    stream.map(|s| {
        std::thread::spawn(move || {
            let reader = std::io::BufReader::new(s);
            for line in reader.lines() {
                let line = line.map_err(RuntimeError::Io)?;
                cb([indent.as_str(), line.as_str()].concat());
            }
            Ok(())
        })
    })
}

fn write_output_lines(output: &mut Output, data: &[u8], indent: &str) -> Result<(), RuntimeError> {
    for line in std::io::BufReader::new(data).lines() {
        let line = line.map_err(RuntimeError::Io)?;
        output
            .writeln(&[indent, &line].concat())
            .map_err(RuntimeError::Io)?;
    }
    Ok(())
}
