use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[derive(Debug)]
pub(crate) struct ShellVarValue {
    pub(crate) stdout: String,
}

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

pub(crate) fn run_captured(
    command: &str,
    dir: Option<&Path>,
    env: Option<&std::collections::HashMap<String, String>>,
    timeout: Option<Duration>,
) -> Result<ShellVarValue, Error> {
    let shell = std::env::var("SHELL").unwrap_or_else(|_| "sh".to_string());
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

    let (status, stdout_buf, stderr_buf) = match timeout {
        Some(dur) => {
            let start = Instant::now();

            let stdout_buf = Arc::new(Mutex::new(Vec::new()));
            let stderr_buf = Arc::new(Mutex::new(Vec::new()));

            let stdout_reader = child.stdout.take().map(|s| {
                let buf = Arc::clone(&stdout_buf);
                std::thread::spawn(move || {
                    let mut tmp = Vec::new();
                    let _ = std::io::BufReader::new(s).read_to_end(&mut tmp);
                    buf.lock().unwrap_or_else(|e| e.into_inner()).extend(tmp);
                })
            });
            let stderr_reader = child.stderr.take().map(|s| {
                let buf = Arc::clone(&stderr_buf);
                std::thread::spawn(move || {
                    let mut tmp = Vec::new();
                    let _ = std::io::BufReader::new(s).read_to_end(&mut tmp);
                    buf.lock().unwrap_or_else(|e| e.into_inner()).extend(tmp);
                })
            });

            let status = loop {
                match child.try_wait() {
                    Ok(Some(status)) => break status,
                    Ok(None) => {
                        if start.elapsed() >= dur {
                            let _ = child.kill();
                            let _ = child.wait();
                            if let Some(h) = stdout_reader {
                                let _ = h.join();
                            }
                            if let Some(h) = stderr_reader {
                                let _ = h.join();
                            }
                            let out = stdout_buf.lock().unwrap_or_else(|e| e.into_inner());
                            let err = stderr_buf.lock().unwrap_or_else(|e| e.into_inner());
                            return Err(Error::Timeout {
                                command: command.to_string(),
                                partial_stdout: String::from_utf8_lossy(&out).into_owned(),
                                partial_stderr: String::from_utf8_lossy(&err).into_owned(),
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

            if let Some(h) = stdout_reader {
                let _ = h.join();
            }
            if let Some(h) = stderr_reader {
                let _ = h.join();
            }

            let out = stdout_buf.lock().unwrap_or_else(|e| e.into_inner()).clone();
            let err = stderr_buf.lock().unwrap_or_else(|e| e.into_inner()).clone();

            (status, out, err)
        }
        None => {
            let output = child.wait_with_output().map_err(Error::Spawn)?;
            (output.status, output.stdout, output.stderr)
        }
    };

    let stdout = String::from_utf8_lossy(&stdout_buf).trim_end().to_string();
    let stderr = String::from_utf8_lossy(&stderr_buf).to_string();

    if !status.success() {
        return Err(Error::Exit {
            command: command.to_string(),
            exit_code: status.code(),
            stderr: stderr.trim().to_string(),
        });
    }

    Ok(ShellVarValue { stdout })
}
