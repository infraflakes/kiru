use super::colors;
use crate::compiler::{Project, ResolvedEnvPair, ResolvedFnStmt};
use crate::runner::error::RuntimeError;
use crate::shell;
use std::collections::HashMap;
use std::io::{self, BufRead, Write};
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::Arc;

/// A writer that implements `Send` for use across thread boundaries.
type SendWriter = Box<dyn Write + Send>;

/// Callback invoked for each line of output (used by the TUI).
pub type OutputCallback = Arc<dyn Fn(String) + Send + Sync>;

/// Where function output is directed: a direct writer or a callback.
pub(crate) enum OutputTarget {
    Direct(SendWriter),
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

    /// Clone the callback if this target is a callback variant.
    pub(crate) fn clone_callback(&self) -> Option<OutputCallback> {
        match self {
            OutputTarget::Callback(cb) => Some(Arc::clone(cb)),
            OutputTarget::Direct(_) => None,
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
    /// directory otherwise.
    pub(crate) fn new(project: Option<&'a Project>, output: &'a mut OutputTarget) -> Self {
        let work_dir = project.map(|p| PathBuf::from(&p.dir)).unwrap_or_else(|| {
            std::env::current_dir().expect("current directory has been deleted or is inaccessible")
        });
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
        let result = self.exec_resolved_fn_body(body);
        self.env_stack.pop();
        result
    }

    pub(super) fn exec_command(&mut self, cmd_str: &str) -> Result<(), RuntimeError> {
        let indent = self.compute_indent_string(0);
        let line = format!("{}exec {}", indent, cmd_str);
        self.output
            .writeln_colored(&line, colors::EXEC_ANSI)
            .map_err(RuntimeError::Io)?;

        let shell = shell::get_current_shell_path();
        let child = Command::new(&shell)
            .arg("-c")
            .arg(cmd_str)
            .current_dir(&self.work_dir)
            .envs(self.build_env_iter())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| RuntimeError::exec_io_error(cmd_str, e))?;

        let indent_str = self.compute_indent_string(1);

        let status = match self.output.clone_callback() {
            Some(cb) => wait_for_callback_output(child, indent_str, cb, cmd_str)?,
            None => wait_for_direct_output(child, self.output, &indent_str, cmd_str)?,
        };

        if !status.success() {
            return Err(RuntimeError::exec_exit_code(cmd_str, status.code()));
        }

        Ok(())
    }
}

/// Wait for a child process to complete, streaming its output through the
/// output callback via background reader threads.
fn wait_for_callback_output(
    mut child: std::process::Child,
    indent: String,
    cb: OutputCallback,
    label: &str,
) -> Result<std::process::ExitStatus, RuntimeError> {
    let stdout_thread = spawn_stream_reader(child.stdout.take(), indent.clone(), cb.clone());
    let stderr_thread = spawn_stream_reader(child.stderr.take(), indent, cb);

    let status = child
        .wait()
        .map_err(|e| RuntimeError::exec_io_error(label, e))?;

    if let Some(result) = stdout_thread.map(|h| h.join()) {
        result.map_err(|_| RuntimeError::Panic("stdout reader panicked".to_string()))??;
    }
    if let Some(result) = stderr_thread.map(|h| h.join()) {
        result.map_err(|_| RuntimeError::Panic("stderr reader panicked".to_string()))??;
    }
    Ok(status)
}

/// Wait for a child process to complete, buffering its output and writing
/// lines directly to the output target.
fn wait_for_direct_output(
    child: std::process::Child,
    output: &mut OutputTarget,
    indent: &str,
    label: &str,
) -> Result<std::process::ExitStatus, RuntimeError> {
    let cmd_output = child
        .wait_with_output()
        .map_err(|e| RuntimeError::exec_io_error(label, e))?;
    write_output_lines(output, &cmd_output.stdout, indent)?;
    write_output_lines(output, &cmd_output.stderr, indent)?;
    Ok(cmd_output.status)
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

/// Spawn a thread that reads lines from a child process stream and sends them
/// to the output callback.  Returns `None` when the stream is `None`.
fn spawn_stream_reader<R: std::io::Read + Send + 'static>(
    stream: Option<R>,
    indent: String,
    cb: OutputCallback,
) -> Option<std::thread::JoinHandle<Result<(), RuntimeError>>> {
    stream.map(|child_stream| {
        std::thread::spawn(move || {
            let reader = std::io::BufReader::new(child_stream);
            for line_result in reader.lines() {
                let line_text = line_result.map_err(RuntimeError::Io)?;
                cb([indent.as_str(), line_text.as_str()].concat());
            }
            Ok(())
        })
    })
}

/// Write captured stdout/stderr lines to the output target.
fn write_output_lines(
    output: &mut OutputTarget,
    data: &[u8],
    indent: &str,
) -> Result<(), RuntimeError> {
    for line_result in std::io::BufReader::new(data).lines() {
        let line_text = line_result.map_err(RuntimeError::Io)?;
        output
            .writeln(&[indent, &line_text].concat())
            .map_err(RuntimeError::Io)?;
    }
    Ok(())
}
