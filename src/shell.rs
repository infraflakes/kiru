use std::io::Read;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

#[derive(Debug)]
pub struct Output {
    pub stdout: String,
}

#[derive(Debug)]
pub enum Error {
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

pub fn run_captured(
    shell: &str,
    command: &str,
    dir: Option<&Path>,
    env: Option<&std::collections::HashMap<String, String>>,
    timeout: Option<Duration>,
) -> Result<Output, Error> {
    let mut cmd = Command::new(shell);
    cmd.arg("-c")
        .arg(command)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());

    if let Some(dir) = dir {
        cmd.current_dir(dir);
    }
    if let Some(env) = env {
        cmd.envs(env);
    }

    let mut child = cmd.spawn().map_err(Error::Spawn)?;

    let status = match timeout {
        Some(dur) => {
            let start = Instant::now();
            loop {
                match child.try_wait() {
                    Ok(Some(status)) => break status,
                    Ok(None) => {
                        if start.elapsed() >= dur {
                            let _ = child.kill();
                            let _ = child.wait();
                            let (out, err) = read_output(&mut child).unwrap_or_else(|e| {
                                eprintln!("[kiru] warning: failed to read output after timeout: {}", e);
                                (Vec::new(), Vec::new())
                            });
                            return Err(Error::Timeout {
                                command: command.to_string(),
                                partial_stdout: String::from_utf8_lossy(&out).into_owned(),
                                partial_stderr: String::from_utf8_lossy(&err).into_owned(),
                            });
                        }
                        std::thread::sleep(Duration::from_millis(50));
                    }
                    Err(e) => return Err(Error::Spawn(e)),
                }
            }
        }
        None => child.wait().map_err(Error::Spawn)?,
    };

    let (stdout_buf, stderr_buf) = read_output(&mut child).map_err(|e| Error::Exit {
        command: command.to_string(),
        exit_code: None,
        stderr: format!("failed to read output: {}", e),
    })?;
    let stdout = String::from_utf8_lossy(&stdout_buf).trim_end().to_string();
    let stderr = String::from_utf8_lossy(&stderr_buf).to_string();

    if !status.success() {
        return Err(Error::Exit {
            command: command.to_string(),
            exit_code: status.code(),
            stderr: stderr.trim().to_string(),
        });
    }

    Ok(Output { stdout })
}

fn read_output(child: &mut Child) -> std::io::Result<(Vec<u8>, Vec<u8>)> {
    let mut stdout_buf = Vec::new();
    let mut stderr_buf = Vec::new();
    if let Some(ref mut s) = child.stdout {
        s.read_to_end(&mut stdout_buf)?;
    }
    if let Some(ref mut s) = child.stderr {
        s.read_to_end(&mut stderr_buf)?;
    }
    Ok((stdout_buf, stderr_buf))
}
