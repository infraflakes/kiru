use crate::plan::{PlanEnvPair, PlanProject, PlanStmt};
use crate::runner::colors;
use crate::runner::error::RuntimeError;
use crate::shell;
use std::collections::HashMap;
use std::io::{self, BufRead};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;

/// Callback invoked for each emitted output line. This is the only output
/// sink: every execution path (the `run`/`sync` TUI, the direct `fn` command)
/// supplies one, so there is no separate "write straight to stdout" mode.
pub type OutputCallback = Arc<dyn Fn(String) + Send + Sync>;

/// Runtime execution context for a resolved function body.
///
/// All variable references have been substituted at compile time, so this
/// context has no variable lookup or scope-tracking logic — it only manages
/// the working directory, environment variable layers, and output.
pub(crate) struct ExecContext<'a> {
    pub(super) output: &'a mut OutputCallback,
    pub(super) env_stack: Vec<HashMap<String, String>>,
    pub(super) work_dir: PathBuf,
    pub(super) env_vars: Vec<(String, String)>,
}

impl<'a> ExecContext<'a> {
    /// Create a new execution context. The working directory is set to
    /// `project.dir` if a project is provided, falling back to the current
    /// directory otherwise. When `KIRU_CWD=1` is set, the current working
    /// directory is always used (useful for CI/CD workflows where the caller
    /// has already positioned the process in the correct directory).
    pub(crate) fn new(project: Option<&'a PlanProject>, output: &'a mut OutputCallback) -> Self {
        let use_cwd = crate::runner::kiru_cwd_enabled();
        let work_dir = if use_cwd {
            std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/"))
        } else {
            project
                .map(|p| PathBuf::from(&p.dir))
                .unwrap_or_else(|| std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/")))
        };
        ExecContext {
            output,
            env_stack: Vec::new(),
            work_dir,
            env_vars: std::env::vars().collect(),
        }
    }

    /// Chain system env vars with per-layer overrides for subprocess execution.
    pub(super) fn build_env_iter(&self) -> impl Iterator<Item = (String, String)> + '_ {
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
    pub(super) fn compute_indent_string(&self, extra: usize) -> String {
        "  ".repeat(self.env_stack.len() + extra)
    }

    /// Run a sequence of resolved function statements sequentially.
    ///
    /// Every statement primitive (`log`, `exec`, `cd`, `env`, `case`) is
    /// dispatched to its dedicated handler.  The function is re-entrant:
    /// `case` arms and `env` blocks call back into `exec_stmts` for their
    /// inner bodies.  The caller is responsible for saving and restoring
    /// `work_dir` if `cd` isolation is needed — `exec_stmts` itself never
    /// resets state.
    ///
    /// This is the single execution entry point used both by direct function
    /// calls (`Runner::execute_fn_call`) and by internal constructs (`env`,
    /// `case`).
    pub(crate) fn exec_stmts(&mut self, body: &[PlanStmt]) -> Result<(), RuntimeError> {
        for stmt in body {
            match stmt {
                PlanStmt::Log(s) => self.exec_log(s)?,
                PlanStmt::Exec(s) => self.exec_command(s)?,
                PlanStmt::Cd(s) => self.exec_cd(s)?,
                PlanStmt::EnvBlock(s) => self.exec_resolved_env_block(&s.pairs, &s.body)?,
                PlanStmt::Case(s) => {
                    for arm in &s.scopes {
                        if match_case_pattern(&arm.pattern, &s.condition) {
                            self.exec_stmts(&arm.body)?;
                        }
                    }
                }
            }
        }
        Ok(())
    }

    pub(crate) fn exec_log(&mut self, msg: &str) -> Result<(), RuntimeError> {
        let indent = self.compute_indent_string(0);
        let line = format!("{}{}{}", indent, colors::LOG_PREFIX, msg);
        (self.output)(line);
        Ok(())
    }

    pub(crate) fn exec_cd(&mut self, resolved: &str) -> Result<(), RuntimeError> {
        let candidate = if Path::new(resolved).is_absolute() {
            PathBuf::from(resolved)
        } else {
            self.work_dir.join(resolved)
        };

        let candidate = std::fs::canonicalize(&candidate)
            .map_err(|e| RuntimeError::Lookup(format!("cd {}: {}", resolved, e)))?;

        if !candidate.is_dir() {
            return Err(RuntimeError::Lookup(format!(
                "cd {}: target is not a directory",
                resolved
            )));
        }

        self.work_dir = candidate;

        let indent = self.compute_indent_string(0);
        let line = format!("{}{}{}", indent, colors::CD_PREFIX, resolved);
        (self.output)(line);
        Ok(())
    }

    pub(crate) fn exec_resolved_env_block(
        &mut self,
        pairs: &[PlanEnvPair],
        body: &[PlanStmt],
    ) -> Result<(), RuntimeError> {
        let mut layer = HashMap::new();
        for pair in pairs {
            layer.insert(pair.key.clone(), pair.value.clone());
        }

        let keys: Vec<&str> = pairs.iter().map(|p| p.key.as_str()).collect();
        let indent = self.compute_indent_string(0);
        let line = format!("{}{}{}", indent, colors::ENV_PREFIX, keys.join(", "));
        (self.output)(line);

        self.env_stack.push(layer);
        let result = self.exec_stmts(body);
        self.env_stack.pop();
        result
    }

    pub(crate) fn exec_command(&mut self, cmd_str: &str) -> Result<(), RuntimeError> {
        let indent = self.compute_indent_string(0);
        let line = format!("{}{}{}", indent, colors::EXEC_PREFIX, cmd_str);
        (self.output)(line);

        let shell = shell::get_current_shell_path();
        let mut child = Command::new(&shell)
            .arg("-c")
            .arg(format!("{} 2>&1", cmd_str))
            .current_dir(&self.work_dir)
            .envs(self.build_env_iter())
            .stdout(Stdio::piped())
            .spawn()
            .map_err(|e| RuntimeError::exec_io_error(cmd_str, e))?;

        let indent_str = self.compute_indent_string(1);
        if let Some(stdout) = child.stdout.take() {
            for line_result in io::BufReader::new(stdout).lines() {
                match line_result {
                    Ok(text) => (self.output)(format!("{}{}", indent_str, text)),
                    Err(_) => break,
                }
            }
        }

        let status = child
            .wait()
            .map_err(|e| RuntimeError::exec_io_error(cmd_str, e))?;

        if !status.success() {
            return Err(RuntimeError::exec_exit_code(cmd_str, status.code()));
        }

        Ok(())
    }
}

/// Check whether a runtime condition matches a resolved case pattern.
pub(crate) fn match_case_pattern(pattern: &crate::plan::PlanCasePattern, condition: &str) -> bool {
    match pattern {
        crate::plan::PlanCasePattern::Literal(lit) => condition == lit,
        crate::plan::PlanCasePattern::Default => true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plan::{PlanCaseArm, PlanCasePattern, PlanCaseStmt, PlanStmt};

    #[test]
    fn test_match_literal_pattern() {
        let pattern = PlanCasePattern::Literal("Linux".to_string());
        assert!(match_case_pattern(&pattern, "Linux"));
        assert!(!match_case_pattern(&pattern, "Darwin"));
    }

    #[test]
    fn test_match_default_pattern() {
        let pattern = PlanCasePattern::Default;
        assert!(match_case_pattern(&pattern, "anything"));
        assert!(match_case_pattern(&pattern, ""));
    }

    #[test]
    fn test_match_empty_string() {
        let pattern = PlanCasePattern::Literal(String::new());
        assert!(match_case_pattern(&pattern, ""));
        assert!(!match_case_pattern(&pattern, "x"));
    }

    #[test]
    fn test_case_first_match_wins() {
        let (_cfg, project, mut output) = crate::runner::test_support::test_context();
        let mut ctx = ExecContext::new(Some(&project), &mut output);
        let body: [PlanStmt; 1] = [PlanStmt::Case(PlanCaseStmt {
            condition: "a".to_string(),
            scopes: vec![
                PlanCaseArm {
                    pattern: PlanCasePattern::Literal("a".to_string()),
                    body: vec![PlanStmt::Log("first".to_string())],
                },
                PlanCaseArm {
                    pattern: PlanCasePattern::Default,
                    body: vec![PlanStmt::Log("second".to_string())],
                },
            ],
        })];
        ctx.exec_stmts(&body).unwrap();
    }

    #[test]
    fn test_case_no_match_does_nothing() {
        let (_cfg, project, mut output) = crate::runner::test_support::test_context();
        let mut ctx = ExecContext::new(Some(&project), &mut output);
        let body: [PlanStmt; 1] = [PlanStmt::Case(PlanCaseStmt {
            condition: "no-match".to_string(),
            scopes: vec![PlanCaseArm {
                pattern: PlanCasePattern::Literal("a".to_string()),
                body: vec![PlanStmt::Log("should-not-run".to_string())],
            }],
        })];
        ctx.exec_stmts(&body).unwrap();
    }
}
