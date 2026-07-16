use super::colors;
use crate::compiler::{Project, ResolvedEnvPair, ResolvedFnStmt};
use crate::runner::error::RuntimeError;
use crate::shell;
use std::collections::HashMap;
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;

/// Callback invoked for each line of output (used by the TUI).
pub type OutputCallback = Arc<dyn Fn(String) + Send + Sync>;

/// Where function output is directed: a direct writer or a callback.
pub(crate) enum OutputTarget {
    Direct(Box<dyn Write + Send>),
    Callback(OutputCallback),
}

impl OutputTarget {
    pub(super) fn writeln(&mut self, content: &str) -> io::Result<()> {
        match self {
            OutputTarget::Direct(w) => writeln!(w, "{content}"),
            OutputTarget::Callback(cb) => {
                cb(content.to_string());
                Ok(())
            }
        }
    }

    pub(super) fn writeln_colored(&mut self, content: &str, color: &str) -> io::Result<()> {
        match self {
            OutputTarget::Direct(w) => writeln!(w, "{color}{content}{}", colors::RESET),
            OutputTarget::Callback(cb) => {
                cb(content.to_string());
                Ok(())
            }
        }
    }
}

/// Runtime execution context for a resolved function body.
///
/// All variable references have been substituted at compile time, so this
/// context has no variable lookup or scope-tracking logic — it only manages
/// the working directory, environment variable layers, and output.
pub(crate) struct ExecContext<'a> {
    pub(super) output: &'a mut OutputTarget,
    pub(super) env_stack: Vec<HashMap<String, String>>,
    pub(super) work_dir: PathBuf,
    pub(super) env_vars: Vec<(String, String)>,
}

impl<'a> ExecContext<'a> {
    /// Create a new execution context. The working directory is set to
    /// `project.dir` if a project is provided, falling back to the current
    /// directory otherwise.  When `KIRU_CWD=1` is set, the current working
    /// directory is always used (useful for CI/CD workflows).
    pub(crate) fn new(project: Option<&'a Project>, output: &'a mut OutputTarget) -> Self {
        let use_cwd = std::env::var("KIRU_CWD").as_deref() == Ok("1");
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
    pub(crate) fn exec_stmts(&mut self, body: &[ResolvedFnStmt]) -> Result<(), RuntimeError> {
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
                            let result = self.exec_stmts(&arm.body);
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
        let indent = self.compute_indent_string(0);
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

        self.work_dir = candidate;

        let indent = self.compute_indent_string(0);
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
        let indent = self.compute_indent_string(0);
        let line = format!("{}env  {}", indent, keys.join(", "));

        self.output
            .writeln_colored(&line, colors::ENV_ANSI)
            .map_err(RuntimeError::Io)?;

        self.env_stack.push(layer);
        let result = self.exec_stmts(body);
        self.env_stack.pop();
        result
    }

    /// Write each line of `data` as indented output lines.
    fn process_output_lines(&mut self, data: &[u8], indent_str: &str) -> Result<(), RuntimeError> {
        for line_result in io::BufReader::new(data).lines() {
            let line_text = line_result.map_err(RuntimeError::Io)?;
            self.output
                .writeln(&[indent_str, &line_text].concat())
                .map_err(RuntimeError::Io)?;
        }
        Ok(())
    }

    pub(super) fn exec_command(&mut self, cmd_str: &str) -> Result<(), RuntimeError> {
        let indent = self.compute_indent_string(0);
        let line = format!("{}exec {}", indent, cmd_str);
        self.output
            .writeln_colored(&line, colors::EXEC_ANSI)
            .map_err(RuntimeError::Io)?;

        let shell = shell::get_current_shell_path();
        let output = Command::new(&shell)
            .arg("-c")
            .arg(cmd_str)
            .current_dir(&self.work_dir)
            .envs(self.build_env_iter())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| RuntimeError::exec_io_error(cmd_str, e))?
            .wait_with_output()
            .map_err(|e| RuntimeError::exec_io_error(cmd_str, e))?;

        let indent_str = self.compute_indent_string(1);
        self.process_output_lines(&output.stdout, &indent_str)?;
        self.process_output_lines(&output.stderr, &indent_str)?;

        if !output.status.success() {
            return Err(RuntimeError::exec_exit_code(cmd_str, output.status.code()));
        }

        Ok(())
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compiler::types::ResolvedCaseArm;
    use crate::compiler::{ResolvedCasePattern, ResolvedFnStmt};

    #[test]
    fn test_match_literal_pattern() {
        let pattern = ResolvedCasePattern::Literal("Linux".to_string());
        assert!(match_case_pattern(&pattern, "Linux"));
        assert!(!match_case_pattern(&pattern, "Darwin"));
    }

    #[test]
    fn test_match_default_pattern() {
        let pattern = ResolvedCasePattern::Default;
        assert!(match_case_pattern(&pattern, "anything"));
        assert!(match_case_pattern(&pattern, ""));
    }

    #[test]
    fn test_match_empty_string() {
        let pattern = ResolvedCasePattern::Literal(String::new());
        assert!(match_case_pattern(&pattern, ""));
        assert!(!match_case_pattern(&pattern, "x"));
    }

    #[test]
    fn test_case_first_match_wins() {
        let (_cfg, project, mut output) = crate::runner::test_support::test_context();
        let mut ctx = ExecContext::new(Some(&project), &mut output);
        let body = [ResolvedFnStmt::Case {
            condition: "a".to_string(),
            scopes: vec![
                ResolvedCaseArm {
                    pattern: ResolvedCasePattern::Literal("a".to_string()),
                    body: vec![ResolvedFnStmt::Log {
                        value: "first".to_string(),
                    }],
                },
                ResolvedCaseArm {
                    pattern: ResolvedCasePattern::Default,
                    body: vec![ResolvedFnStmt::Log {
                        value: "second".to_string(),
                    }],
                },
            ],
        }];
        ctx.exec_stmts(&body).unwrap();
    }

    #[test]
    fn test_case_no_match_does_nothing() {
        let (_cfg, project, mut output) = crate::runner::test_support::test_context();
        let mut ctx = ExecContext::new(Some(&project), &mut output);
        let body = [ResolvedFnStmt::Case {
            condition: "no-match".to_string(),
            scopes: vec![ResolvedCaseArm {
                pattern: ResolvedCasePattern::Literal("a".to_string()),
                body: vec![ResolvedFnStmt::Log {
                    value: "should-not-run".to_string(),
                }],
            }],
        }];
        ctx.exec_stmts(&body).unwrap();
    }
}
