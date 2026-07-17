use crate::compiler::error::CompileError;
use crate::error::{SourceFile, spanned_report};
use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

const COMMAND_TIMEOUT: Duration = Duration::from_secs(30);
const POLL_INTERVAL: Duration = Duration::from_millis(50);

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

/// Spawn a thread that reads a child process stdout/stderr pipe to EOF and
/// appends every byte into a shared `Mutex<Vec<u8>>` buffer.
fn spawn_reader_thread<T: Read + Send + 'static>(
    child_stream: Option<T>,
    buffer: Arc<Mutex<Vec<u8>>>,
) -> Option<JoinHandle<()>> {
    child_stream.map(|stream| {
        std::thread::spawn(move || {
            let mut read_buffer = Vec::new();
            let _ = std::io::BufReader::new(stream).read_to_end(&mut read_buffer);
            buffer
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .extend(read_buffer);
        })
    })
}

/// Wait for a child process to finish, polling with a timeout.
/// Returns the exit status on success, or a `Timeout`/`Spawn` error.
/// On timeout the reader threads are joined and their partial output
/// captured in the error.
fn poll_child_with_timeout(
    child: &mut std::process::Child,
    command: &str,
    start: Instant,
    stdout_reader: &mut Option<JoinHandle<()>>,
    stderr_reader: &mut Option<JoinHandle<()>>,
    stdout_buf: &Arc<Mutex<Vec<u8>>>,
    stderr_buf: &Arc<Mutex<Vec<u8>>>,
) -> Result<std::process::ExitStatus, Error> {
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Ok(status),
            Ok(None) => {
                if start.elapsed() >= COMMAND_TIMEOUT {
                    let _ = child.kill();
                    let _ = child.wait();
                    if let Some(h) = stdout_reader.take() {
                        let _ = h.join();
                    }
                    if let Some(h) = stderr_reader.take() {
                        let _ = h.join();
                    }
                    let so = stdout_buf.lock().unwrap_or_else(|e| e.into_inner());
                    let se = stderr_buf.lock().unwrap_or_else(|e| e.into_inner());
                    return Err(Error::Timeout {
                        command: command.to_string(),
                        partial_stdout: String::from_utf8_lossy(&so).into_owned(),
                        partial_stderr: String::from_utf8_lossy(&se).into_owned(),
                    });
                }
                std::thread::sleep(POLL_INTERVAL);
            }
            Err(e) => {
                let _ = child.kill();
                let _ = child.wait();
                return Err(Error::Spawn(e));
            }
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

    let start = Instant::now();

    let stdout_buf = Arc::new(Mutex::new(Vec::new()));
    let stderr_buf = Arc::new(Mutex::new(Vec::new()));

    let mut stdout_reader = spawn_reader_thread(child.stdout.take(), Arc::clone(&stdout_buf));
    let mut stderr_reader = spawn_reader_thread(child.stderr.take(), Arc::clone(&stderr_buf));

    let status = poll_child_with_timeout(
        &mut child,
        command,
        start,
        &mut stdout_reader,
        &mut stderr_reader,
        &stdout_buf,
        &stderr_buf,
    )?;

    if let Some(h) = stdout_reader {
        let _ = h.join();
    }
    if let Some(h) = stderr_reader {
        let _ = h.join();
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
///
/// `working_dir` — when `Some`, runs the command in that directory;
/// when `None`, runs in the current process directory.
pub(crate) fn execute_shell_variable(
    name: &str,
    resolved_command: &str,
    working_dir: Option<&Path>,
    source: &SourceFile<'_>,
    offset: usize,
    len: usize,
) -> Result<String, CompileError> {
    match exec_and_get_stdout(resolved_command, working_dir, None) {
        Ok(stdout) => Ok(stdout),
        // Non-zero exit is not an error — empty string is a valid value
        // in Kiru's type system.
        Err(Error::Exit { .. }) => Ok(String::new()),
        Err(e) => Err(CompileError::ValidationReport(vec![spanned_report(
            format!("shell var ${} failed: {}", name, e),
            source,
            offset,
            len,
        )])),
    }
}

/// Return the user's shell from `$SHELL`, defaulting to `"sh"`.
pub(crate) fn get_current_shell_path() -> String {
    std::env::var("SHELL").unwrap_or_else(|_| "sh".to_string())
}
