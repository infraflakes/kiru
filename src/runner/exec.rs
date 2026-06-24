use std::io::BufRead;
use std::io::Read;
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use super::colors;
use crate::dsl::Expr;
use crate::runner::Output;
use crate::runner::OutputCallback;
use crate::runner::error::RuntimeError;

use super::parse::ExecContext;

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

pub(crate) fn exec_and_get_stdout(
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
        if status.code().is_none() {
            std::process::exit(130);
        }
        return Err(Error::Exit {
            command: command.to_string(),
            exit_code: status.code(),
            stderr: stderr.trim().to_string(),
        });
    }

    Ok(ShellVarValue { stdout })
}

fn current_shell() -> String {
    std::env::var("SHELL").unwrap_or_else(|_| "sh".to_string())
}

impl ExecContext<'_> {
    pub(super) fn exec_command(&mut self, value: &Expr) -> Result<(), RuntimeError> {
        let cmd_str = self.resolve_expr(value)?;
        let indent = self.indent(0);
        let line = format!("{}exec {}", indent, cmd_str);
        self.output
            .writeln_colored(&line, colors::EXEC_ANSI)
            .map_err(RuntimeError::Io)?;

        let shell = current_shell();
        let mut child = Command::new(&shell)
            .arg("-c")
            .arg(&cmd_str)
            .current_dir(&self.work_dir)
            .envs(self.build_env())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|e| RuntimeError::exec_io_error(&cmd_str, e))?;

        let indent = self.indent(1);

        let status = match self.output.clone_callback() {
            Some(cb) => {
                let stdout_thread =
                    spawn_stream_reader(child.stdout.take(), indent.clone(), cb.clone());
                let stderr_thread = spawn_stream_reader(child.stderr.take(), indent, cb);

                let status = child
                    .wait()
                    .map_err(|e| RuntimeError::exec_io_error(&cmd_str, e))?;

                if let Some(result) = stdout_thread.map(|h| h.join()) {
                    result
                        .map_err(|_| RuntimeError::Panic("stdout reader panicked".to_string()))??;
                }
                if let Some(result) = stderr_thread.map(|h| h.join()) {
                    result
                        .map_err(|_| RuntimeError::Panic("stderr reader panicked".to_string()))??;
                }

                status
            }
            None => {
                let output = child
                    .wait_with_output()
                    .map_err(|e| RuntimeError::exec_io_error(&cmd_str, e))?;
                write_output_lines(self.output, &output.stdout, &indent)?;
                write_output_lines(self.output, &output.stderr, &indent)?;
                output.status
            }
        };

        if !status.success() {
            if status.code().is_none() {
                std::process::exit(130);
            }
            return Err(RuntimeError::exec_exit_code(cmd_str, status.code()));
        }

        Ok(())
    }
}

fn spawn_stream_reader<R: std::io::Read + Send + 'static>(
    stream: Option<R>,
    indent: String,
    cb: OutputCallback,
) -> Option<std::thread::JoinHandle<Result<(), RuntimeError>>> {
    stream.map(|s| {
        std::thread::spawn(move || {
            let reader = std::io::BufReader::new(s);
            for line in reader.lines() {
                let line = line.map_err(RuntimeError::Io)?;
                cb([indent.as_str(), line.as_str()].concat());
            }
            Ok(())
        })
    })
}

fn write_output_lines(output: &mut Output, data: &[u8], indent: &str) -> Result<(), RuntimeError> {
    for line in std::io::BufReader::new(data).lines() {
        let line = line.map_err(RuntimeError::Io)?;
        output
            .writeln(&[indent, &line].concat())
            .map_err(RuntimeError::Io)?;
    }
    Ok(())
}
