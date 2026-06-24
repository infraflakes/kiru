use super::colors;
use super::exec;
use crate::compiler::{Config, Project};
use crate::dsl::{CaseMatch, Expr, FnStmt};
use crate::runner::Output;
use crate::runner::error::RuntimeError;
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

pub type OutputCallback = Arc<dyn Fn(String) + Send + Sync>;

pub(crate) struct ExecContext<'a> {
    pub(super) cfg: &'a Config,
    pub(super) project: Option<&'a Project>,
    pub(crate) output: &'a mut Output,
    /// Base variables (global + project string vars + cached shell var results).
    pub(super) vars: HashMap<String, String>,
    /// Unresolved shell var commands (global + project). Executed on first access.
    pub(super) shell_vars: HashMap<String, String>,
    /// Scope stack for variable shadowing. Each layer shadows `vars` and higher layers.
    pub(super) var_stack: Vec<HashMap<String, String>>,
    pub(super) env_stack: Vec<HashMap<String, String>>,
    pub(super) work_dir: PathBuf,
    pub(super) env_vars: Vec<(String, String)>,
}

impl<'a> ExecContext<'a> {
    pub(crate) fn new(
        cfg: &'a Config,
        project: Option<&'a Project>,
        output: &'a mut Output,
    ) -> Self {
        let mut vars = cfg.vars.clone();
        let mut shell_vars = cfg.shell_vars.clone();
        if let Some(proj) = project {
            vars.extend(proj.vars.clone());
            shell_vars.extend(proj.shell_vars.clone());
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
            shell_vars,
            var_stack: Vec::new(),
            env_stack: Vec::new(),
            work_dir,
            env_vars: std::env::vars().collect(),
        }
    }

    /// Resolve an expression, checking scope layers before base vars.
    pub(super) fn resolve_expr(&mut self, expr: &Expr) -> Result<String, RuntimeError> {
        match expr {
            Expr::VarRef { name, .. } => self
                .resolve_var(name)?
                .ok_or_else(|| RuntimeError::Lookup(format!("undefined variable: ${}", name))),
            Expr::BacktickLit { parts, .. } => {
                let mut result = String::new();
                for part in parts {
                    if part.is_var {
                        let val = self.resolve_var(&part.value)?.ok_or_else(|| {
                            RuntimeError::Lookup(format!("undefined variable: ${}", part.value))
                        })?;
                        result.push_str(&val);
                    } else {
                        result.push_str(&part.value);
                    }
                }
                Ok(result)
            }
        }
    }

    /// Look up a variable name, checking scope layers (top-to-bottom) then base/shell vars.
    /// Shell vars are lazily executed on first access and cached in `self.vars`.
    fn resolve_var(&mut self, name: &str) -> Result<Option<String>, RuntimeError> {
        for layer in self.var_stack.iter().rev() {
            if let Some(val) = layer.get(name) {
                return Ok(Some(val.clone()));
            }
        }
        if let Some(val) = self.vars.get(name) {
            return Ok(Some(val.clone()));
        }
        if let Some(cmd) = self.shell_vars.remove(name) {
            let env_map: HashMap<String, String> = self.build_env().collect();
            let out = exec::exec_and_get_stdout(&cmd, Some(&self.work_dir), Some(&env_map), None)
                .map_err(|e| match e {
                exec::Error::Spawn(io_err) => RuntimeError::exec_io_error(&cmd, io_err),
                exec::Error::Exit {
                    stderr, exit_code, ..
                } => RuntimeError::Exec {
                    cmd: cmd.clone(),
                    exit_code,
                    detail: stderr,
                },
                exec::Error::Timeout { partial_stderr, .. } => RuntimeError::Exec {
                    cmd: cmd.clone(),
                    exit_code: None,
                    detail: format!("timed out: {}", partial_stderr),
                },
            })?;
            let val = out.stdout;
            self.vars.insert(name.to_string(), val.clone());
            return Ok(Some(val));
        }
        Ok(None)
    }

    pub(super) fn build_env(&self) -> impl Iterator<Item = (String, String)> + '_ {
        let sys = self.env_vars.iter().map(|(k, v)| (k.clone(), v.clone()));
        let overrides = self
            .env_stack
            .iter()
            .flat_map(|layer| layer.iter().map(|(k, v)| (k.clone(), v.clone())));
        sys.chain(overrides)
    }

    pub(super) fn indent(&self, extra: usize) -> String {
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
                FnStmt::Exec { value, .. } => self.exec_command(value)?,
                FnStmt::Cd { value, .. } => self.exec_cd(value)?,
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
                FnStmt::Case { condition, scopes } => {
                    let value = self.resolve_expr(condition)?;
                    for branch in scopes {
                        if self.match_case_pattern(&branch.pattern, &value)? {
                            self.var_stack.push(HashMap::new());
                            let result = self.exec_fn_body(&branch.body);
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
        pattern: &CaseMatch,
        value: &str,
    ) -> Result<bool, RuntimeError> {
        match pattern {
            CaseMatch::Default => Ok(true),
            CaseMatch::Literal { parts } => {
                let mut resolved = String::new();
                for part in parts {
                    if part.is_var {
                        let v = self.resolve_var(&part.value)?.ok_or_else(|| {
                            RuntimeError::Lookup(format!("undefined variable: ${}", part.value))
                        })?;
                        resolved.push_str(&v);
                    } else {
                        resolved.push_str(&part.value);
                    }
                }
                Ok(value == resolved)
            }
            CaseMatch::VarRef { name } => {
                let v = self.resolve_var(name)?.ok_or_else(|| {
                    RuntimeError::Lookup(format!("undefined variable: ${}", name))
                })?;
                Ok(value == v)
            }
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
        var_type: &crate::dsl::VarType,
    ) -> Result<(), RuntimeError> {
        let val = self.resolve_expr(value)?;

        let resolved = if var_type == &crate::dsl::VarType::Shell {
            let env_map: HashMap<String, String> = self.build_env().collect();
            let out = exec::exec_and_get_stdout(&val, Some(&self.work_dir), Some(&env_map), None)
                .map_err(|e| match e {
                exec::Error::Spawn(io_err) => RuntimeError::exec_io_error(&val, io_err),
                exec::Error::Exit {
                    stderr, exit_code, ..
                } => RuntimeError::Exec {
                    cmd: val.clone(),
                    exit_code,
                    detail: stderr,
                },
                exec::Error::Timeout { partial_stderr, .. } => RuntimeError::Exec {
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
        pairs: &[crate::dsl::EnvPair],
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
