use super::colors;
use crate::compiler::{Project, ResolvedEnvPair, ResolvedFnStmt, Sanctuary};
use crate::runner::error::RuntimeError;
use crate::runner::output::OutputTarget;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

pub type OutputCallback = Arc<dyn Fn(String) + Send + Sync>;
use std::sync::Arc;

/// Runtime execution context for a resolved function body.
///
/// All variable references have been substituted at compile time, so this
/// context has no variable lookup or scope-tracking logic — it only manages
/// the working directory, environment variable layers, and output.
pub(crate) struct ExecContext<'a> {
    pub(super) cfg: &'a Sanctuary,
    pub(super) project: Option<&'a Project>,
    pub(super) output: &'a mut OutputTarget,
    pub(super) env_stack: Vec<HashMap<String, String>>,
    pub(super) work_dir: PathBuf,
    pub(super) env_vars: Vec<(String, String)>,
}

impl<'a> ExecContext<'a> {
    pub(crate) fn new(
        cfg: &'a Sanctuary,
        project: Option<&'a Project>,
        output: &'a mut OutputTarget,
    ) -> Self {
        let base = if cfg.sanctuary_path.is_empty() {
            std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
        } else {
            PathBuf::from(&cfg.sanctuary_path)
        };
        let dir = project.map(|p| &*p.dir).unwrap_or("");
        let work_dir = if dir.is_empty() {
            base
        } else {
            base.join(dir.trim_start_matches('/'))
        };
        ExecContext {
            cfg,
            project,
            output,
            env_stack: Vec::new(),
            work_dir,
            env_vars: std::env::vars().collect(),
        }
    }

    /// Chain system env vars with per-layer overrides for subprocess execution.
    pub(super) fn build_env(&self) -> impl Iterator<Item = (String, String)> + '_ {
        let system_env_vars = self
            .env_vars
            .iter()
            .map(|(key, value)| (key.clone(), value.clone()));
        let layer_overrides = self.env_stack.iter().flat_map(|layer| {
            layer
                .iter()
                .map(|(key, value)| (key.clone(), value.clone()))
        });
        system_env_vars.chain(layer_overrides)
    }

    /// Compute indentation string based on current env block nesting depth.
    pub(super) fn indent(&self, extra: usize) -> String {
        "  ".repeat(self.env_stack.len() + extra)
    }

    /// Execute a fully resolved function body.
    /// All values are concrete strings — no variable resolution happens here.
    pub(crate) fn exec_resolved_fn_body(
        &mut self,
        body: &[ResolvedFnStmt],
    ) -> Result<(), RuntimeError> {
        let saved_work_dir = self.work_dir.clone();
        let result = self.exec_resolved_fn_body_inner(body);
        self.work_dir = saved_work_dir;
        result
    }

    fn exec_resolved_fn_body_inner(&mut self, body: &[ResolvedFnStmt]) -> Result<(), RuntimeError> {
        for stmt in body {
            match stmt {
                ResolvedFnStmt::Log { value } => self.exec_log(value)?,
                ResolvedFnStmt::Exec { value } => self.exec_command(value)?,
                ResolvedFnStmt::Cd { value } => self.exec_cd(value)?,
                ResolvedFnStmt::EnvBlock { pairs, body } => {
                    self.exec_resolved_env_block(pairs, body)?;
                }
                ResolvedFnStmt::Case { condition, scopes } => {
                    for arm in scopes {
                        if match_case_pattern(&arm.pattern, condition) {
                            let result = self.exec_resolved_fn_body_inner(&arm.body);
                            result?;
                            break;
                        }
                    }
                }
            }
        }
        Ok(())
    }

    fn exec_log(&mut self, msg: &str) -> Result<(), RuntimeError> {
        let indent = self.indent(0);
        let line = format!("{}log  {}", indent, msg);
        self.output
            .writeln_colored(&line, colors::LOG_ANSI)
            .map_err(RuntimeError::Io)?;
        Ok(())
    }

    fn exec_cd(&mut self, resolved: &str) -> Result<(), RuntimeError> {
        if Path::new(resolved).is_absolute() {
            return Err(RuntimeError::Lookup(format!(
                "cd {}: absolute path not allowed",
                resolved
            )));
        }

        let candidate = self.work_dir.join(resolved);

        let candidate = std::fs::canonicalize(&candidate)
            .map_err(|e| RuntimeError::Lookup(format!("cd {}: {}", resolved, e)))?;

        if !candidate.is_dir() {
            return Err(RuntimeError::Lookup(format!(
                "cd {}: target is not a directory",
                resolved
            )));
        }

        if let Some(current_project) = self.project {
            let base = if self.cfg.sanctuary_path.is_empty() {
                std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."))
            } else {
                PathBuf::from(&self.cfg.sanctuary_path)
            };
            let base_canonical =
                std::fs::canonicalize(base.join(current_project.dir.trim_start_matches('/')))
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

    fn exec_resolved_env_block(
        &mut self,
        pairs: &[ResolvedEnvPair],
        body: &[ResolvedFnStmt],
    ) -> Result<(), RuntimeError> {
        let mut layer = HashMap::new();
        for pair in pairs {
            layer.insert(pair.key.clone(), pair.value.clone());
        }

        let keys: Vec<&str> = pairs.iter().map(|p| p.key.as_str()).collect();
        let indent = self.indent(0);
        let line = format!("{}env  {}", indent, keys.join(", "));

        self.output
            .writeln_colored(&line, colors::ENV_ANSI)
            .map_err(RuntimeError::Io)?;

        self.env_stack.push(layer);
        let result = self.exec_resolved_fn_body(body);
        self.env_stack.pop();
        result
    }
}

/// Check whether a runtime condition matches a resolved case pattern.
pub(crate) fn match_case_pattern(
    pattern: &crate::compiler::ResolvedCasePattern,
    condition: &str,
) -> bool {
    match pattern {
        crate::compiler::ResolvedCasePattern::Literal(lit) => condition == lit,
        crate::compiler::ResolvedCasePattern::Default => true,
    }
}
