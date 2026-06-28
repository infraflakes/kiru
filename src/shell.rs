use crate::compiler::error::{CompileError, spanned_err};
use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

/// Errors from shell command execution.
#[derive(Debug)]
pub(crate) enum Error {
    Spawn(std::io::Error),
    Exit {
        command: String,
        exit_code: Option<i32>,
        stderr: String,
    },
    Timeout {
        command: String,
        partial_stdout: String,
        partial_stderr: String,
    },
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Error::Spawn(e) => write!(f, "failed to spawn shell: {}", e),
            Error::Exit {
                command,
                exit_code,
                stderr,
                ..
            } => match exit_code {
                Some(code) if stderr.is_empty() => {
                    write!(f, "shell command failed: {} (exit code {})", command, code)
                }
                Some(code) => {
                    write!(
                        f,
                        "shell command failed: {}: {} (exit code {})",
                        command, stderr, code
                    )
                }
                None => write!(f, "shell command failed: {}: {}", command, stderr),
            },
            Error::Timeout {
                command,
                partial_stdout,
                partial_stderr,
            } => {
                let detail = if partial_stderr.trim().is_empty() {
                    partial_stdout.trim()
                } else {
                    partial_stderr.trim()
                };
                if detail.is_empty() {
                    write!(f, "shell command timed out: {}", command)
                } else {
                    write!(f, "shell command timed out: {}: {}", command, detail)
                }
            }
        }
    }
}

impl std::error::Error for Error {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Error::Spawn(e) => Some(e),
            _ => None,
        }
    }
}

/// Execute a shell command and capture stdout.  Applies an optional working
/// directory and environment variable overrides.  Times out after 30 seconds.
pub(crate) fn exec_and_get_stdout(
    command: &str,
    working_dir: Option<&Path>,
    env_overrides: Option<&std::collections::HashMap<String, String>>,
) -> Result<String, Error> {
    let shell_path = get_current_shell_path();
    let mut shell_command = Command::new(shell_path);
    shell_command
        .arg("-c")
        .arg(command)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    if let Some(dir) = working_dir {
        shell_command.current_dir(dir);
    }
    if let Some(overrides) = env_overrides {
        shell_command.envs(overrides);
    }

    let mut child = shell_command.spawn().map_err(Error::Spawn)?;

    let timeout_duration = Duration::from_secs(30);
    let start = Instant::now();

    let stdout_buf = Arc::new(Mutex::new(Vec::new()));
    let stderr_buf = Arc::new(Mutex::new(Vec::new()));

    let stdout_reader = child.stdout.take().map(|child_stream| {
        let buffer_clone = Arc::clone(&stdout_buf);
        std::thread::spawn(move || {
            let mut read_buffer = Vec::new();
            let _ = std::io::BufReader::new(child_stream).read_to_end(&mut read_buffer);
            buffer_clone
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .extend(read_buffer);
        })
    });
    let stderr_reader = child.stderr.take().map(|child_stream| {
        let buffer_clone = Arc::clone(&stderr_buf);
        std::thread::spawn(move || {
            let mut read_buffer = Vec::new();
            let _ = std::io::BufReader::new(child_stream).read_to_end(&mut read_buffer);
            buffer_clone
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .extend(read_buffer);
        })
    });

    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) => {
                if start.elapsed() >= timeout_duration {
                    let _ = child.kill();
                    let _ = child.wait();
                    if let Some(join_handle) = stdout_reader {
                        let _ = join_handle.join();
                    }
                    if let Some(join_handle) = stderr_reader {
                        let _ = join_handle.join();
                    }
                    let stdout_guard = stdout_buf.lock().unwrap_or_else(|e| e.into_inner());
                    let stderr_guard = stderr_buf.lock().unwrap_or_else(|e| e.into_inner());
                    return Err(Error::Timeout {
                        command: command.to_string(),
                        partial_stdout: String::from_utf8_lossy(&stdout_guard).into_owned(),
                        partial_stderr: String::from_utf8_lossy(&stderr_guard).into_owned(),
                    });
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(Error::Spawn(e));
            }
        }
    };

    if let Some(join_handle) = stdout_reader {
        let _ = join_handle.join();
    }
    if let Some(join_handle) = stderr_reader {
        let _ = join_handle.join();
    }

    let stdout_data = stdout_buf.lock().unwrap_or_else(|e| e.into_inner()).clone();
    let stderr_data = stderr_buf.lock().unwrap_or_else(|e| e.into_inner()).clone();

    let stdout = String::from_utf8_lossy(&stdout_data).trim_end().to_string();
    let stderr = String::from_utf8_lossy(&stderr_data).to_string();

    if !status.success() {
        return Err(Error::Exit {
            command: command.to_string(),
            exit_code: status.code(),
            stderr: stderr.trim().to_string(),
        });
    }

    Ok(stdout)
}

/// Execute a shell command for a `var shell` statement.
/// Non-zero exit codes produce an empty string (callers use this to
/// gracefully handle failed shell commands during variable resolution).
pub(crate) fn execute_shell_variable(
    name: &str,
    resolved_command: &str,
    source_name: &str,
    source_text: &str,
    offset: usize,
    len: usize,
) -> Result<String, CompileError> {
    match exec_and_get_stdout(resolved_command, None, None) {
        Ok(stdout) => Ok(stdout),
        Err(Error::Exit { .. }) => Ok(String::new()),
        Err(e) => Err(spanned_err(
            format!("shell var ${} failed: {}", name, e),
            source_name,
            source_text,
            offset,
            len,
        )),
    }
}

/// Return the user's shell from `$SHELL`, defaulting to `"sh"`.
pub(crate) fn get_current_shell_path() -> String {
    std::env::var("SHELL").unwrap_or_else(|_| "sh".to_string())
}
