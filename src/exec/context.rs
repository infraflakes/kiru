use super::subprocess;
use crate::exec::colors;
use crate::exec::error::RuntimeError;
use crate::ir::{ArmPattern, EnvPair, Instruction, Segment, Template};
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

/// Callback invoked for each emitted output line. This is the only output
/// sink: every execution path (the `run` and `sync` TUIs) supplies one, so
/// there is no separate "write straight to stdout" mode.
pub(crate) type OutputCallback = Arc<dyn Fn(String) + Send + Sync>;

/// Runtime execution context for a resolved function body.
///
/// Variables are fully inlined at compile time, so there is no runtime scope
/// stack: every template here is literal text and `$(command)` substitutions.
pub(crate) struct ExecContext<'a> {
    output: &'a mut OutputCallback,
    cwd: PathBuf,
    env_layers: Vec<BTreeMap<String, String>>,
    system_env: Vec<(String, String)>,
    shell: String,
    timeout: Option<Duration>,
}

impl<'a> ExecContext<'a> {
    /// Create a new execution context. `cwd` is the starting working directory;
    /// when `timeout` is `None`, commands have no time limit.
    pub(crate) fn new(
        output: &'a mut OutputCallback,
        cwd: PathBuf,
        shell: String,
        timeout: Option<Duration>,
    ) -> Self {
        ExecContext {
            output,
            cwd,
            env_layers: Vec::new(),
            system_env: std::env::vars().collect(),
            shell,
            timeout,
        }
    }

    /// Resolve a template to a string. When `strict` is true, `$(cmd)` parts
    /// are executed live and must succeed (fatal on non-zero exit or timeout);
    /// otherwise their stdout is captured and inlined, tolerantly returning
    /// empty string on error.
    fn resolve(&self, tmpl: &Template, strict: bool) -> Result<String, RuntimeError> {
        let mut out = String::new();
        for segment in &tmpl.parts {
            match segment {
                Segment::Lit(s) => out.push_str(s),
                Segment::Cmd(inner) => {
                    let cmd = self.resolve(inner, false)?;
                    if strict {
                        self.run_live(&cmd)?;
                    } else {
                        match self.capture(&cmd) {
                            Ok(captured) => {
                                if !captured.is_empty() {
                                    out.push_str(&captured);
                                }
                            }
                            Err(RuntimeError::Timeout { .. }) => {
                                // Tolerant mode: inner timeout silently returns
                                // empty string, no propagation.
                            }
                            Err(e) => return Err(e),
                        }
                    }
                }
            }
        }
        Ok(out)
    }

    /// Run a command and stream its output live (fatal on non-zero exit or
    /// timeout). Emits the shell command echo at `log` indent level (0) in
    /// blue, streams output at `log + 1` indent level, and emits timeout
    /// errors via the output callback at the same indent as streaming output.
    fn run_live(&self, cmd: &str) -> Result<(), RuntimeError> {
        let work_dir = &self.cwd;
        let env_overrides: HashMap<String, String> = self.env_overrides();
        let output_indent = "  ".repeat(self.env_layers.len() + 1);
        let shell_indent = "  ".repeat(self.env_layers.len());
        let shell = &self.shell;

        // Echo: "{shell}  {cmd}" in blue at log indent level.
        (self.output)(format!(
            "{shell_indent}{}{shell}  {cmd}{}",
            colors::CMD_ANSI,
            colors::RESET
        ));

        let status = subprocess::run_subprocess(
            cmd,
            &[shell, "-c", cmd],
            Some(work_dir),
            Some(&env_overrides),
            self.timeout,
            &mut |line| match line {
                subprocess::SubprocessLine::Stdout(text)
                | subprocess::SubprocessLine::Stderr(text) => {
                    (self.output)(format!("{output_indent}{}", text.trim_start()));
                }
            },
        );
        match status {
            Ok(status) => {
                if !status.success() {
                    return Err(RuntimeError::exec_io_error(
                        cmd,
                        subprocess::describe_exit_failure(&status),
                    ));
                }
                Ok(())
            }
            Err(subprocess::SubprocessError::Timeout { command, .. }) => {
                let timeout_secs = self.timeout.map_or(0, |d| d.as_secs());
                (self.output)(format!(
                    "{output_indent}Error: timeout: command timed out after {timeout_secs}s: {command}"
                ));
                Err(RuntimeError::Timeout {
                    cmd: command,
                    secs: timeout_secs,
                })
            }
            Err(e) => Err(RuntimeError::exec_io_error(cmd, e)),
        }
    }

    /// Run a command and capture its stdout (trimmed). Returns
    /// `Err(Timeout { .. })` when the process exceeds the global timeout; in
    /// that case the captured output so far is discarded. Non-zero exit is
    /// non-fatal.
    fn capture(&self, cmd: &str) -> Result<String, RuntimeError> {
        let env_overrides: HashMap<String, String> = self.env_overrides();
        subprocess::capture_shell(
            cmd,
            &self.shell,
            Some(&self.cwd),
            Some(&env_overrides),
            self.timeout,
        )
        .map_err(|e| match e {
            subprocess::SubprocessError::Timeout { command, .. } => RuntimeError::Timeout {
                cmd: command,
                secs: self.timeout.map_or(0, |d| d.as_secs()),
            },
            other => RuntimeError::exec_io_error(cmd, other),
        })
    }

    /// Combine system env vars with every active env-block layer.
    fn env_overrides(&self) -> HashMap<String, String> {
        let mut env: HashMap<String, String> = self.system_env.iter().cloned().collect();
        for layer in &self.env_layers {
            for (k, v) in layer {
                env.insert(k.clone(), v.clone());
            }
        }
        env
    }

    /// Emit one output line: indent, prefix, then payload.
    fn emit(&mut self, indent_extra: usize, prefix: &str, payload: &str) {
        let indent = "  ".repeat(self.env_layers.len() + indent_extra);
        (self.output)(format!("{indent}{prefix}{payload}"));
    }

    /// Run a sequence of resolved instructions sequentially. This is the single
    /// execution entry point used by both `Executor::execute_fn_call` and the
    /// internal `env`/`switch` constructs (which recurse here for their bodies).
    pub(crate) fn exec_stmts(&mut self, body: &[Instruction]) -> Result<(), RuntimeError> {
        for stmt in body {
            match stmt {
                Instruction::Log(t) => {
                    let resolved = self.resolve(t, false)?;
                    self.emit(0, colors::LOG_PREFIX, &resolved);
                }
                Instruction::RunShellCmd { value } => {
                    // Bare `$(cmd);` from a lowered `exec`, execute for side
                    // effects only; the variable is already inlined everywhere.
                    self.resolve(value, true)?;
                }
                Instruction::Cd(t) => {
                    let target = self.resolve(t, false)?;
                    self.exec_cd(&target)?;
                }
                Instruction::Env { pairs, body } => {
                    self.exec_env_block(pairs, body)?;
                }
                Instruction::Switch { subject, arms } => {
                    let condition = self.resolve(subject, false)?;
                    for arm in arms {
                        let matched = match &arm.pattern {
                            ArmPattern::Lit(s) => s == &condition,
                            ArmPattern::Default => true,
                        };
                        if matched {
                            self.exec_stmts(&arm.body)?;
                            break;
                        }
                    }
                }
            }
        }
        Ok(())
    }

    fn exec_cd(&mut self, target: &str) -> Result<(), RuntimeError> {
        let candidate = if Path::new(target).is_absolute() {
            PathBuf::from(target)
        } else {
            self.cwd.join(target)
        };
        let candidate = std::fs::canonicalize(&candidate)
            .map_err(|e| RuntimeError::Lookup(format!("cd {target}: {e}")))?;
        if !candidate.is_dir() {
            return Err(RuntimeError::Lookup(format!(
                "cd {target}: not a directory"
            )));
        }
        self.cwd = candidate;
        self.emit(0, colors::CD_PREFIX, target);
        Ok(())
    }

    fn exec_env_block(
        &mut self,
        pairs: &[EnvPair],
        body: &[Instruction],
    ) -> Result<(), RuntimeError> {
        let mut layer = BTreeMap::new();
        for pair in pairs {
            layer.insert(pair.key.clone(), self.resolve(&pair.value, false)?);
        }
        let keys: Vec<&str> = pairs.iter().map(|p| p.key.as_str()).collect();
        self.emit(0, colors::ENV_PREFIX, &keys.join(", "));
        self.env_layers.push(layer);
        let result = self.exec_stmts(body);
        self.env_layers.pop();
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::{Arm, Instruction, Template};

    fn lit(s: &str) -> Template {
        Template {
            parts: vec![Segment::Lit(s.to_string())],
        }
    }

    #[test]
    fn test_switch_first_match() {
        let mut output: OutputCallback = Arc::new(|_| {});
        let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/"));
        let mut ctx = ExecContext::new(
            &mut output,
            cwd,
            "sh".to_string(),
            Some(Duration::from_secs(30)),
        );
        let body: [Instruction; 1] = [Instruction::Switch {
            subject: lit("a"),
            arms: vec![
                Arm {
                    pattern: ArmPattern::Lit("a".to_string()),
                    body: vec![Instruction::Log(lit("first"))],
                },
                Arm {
                    pattern: ArmPattern::Default,
                    body: vec![Instruction::Log(lit("second"))],
                },
            ],
        }];
        ctx.exec_stmts(&body).unwrap();
    }
}
