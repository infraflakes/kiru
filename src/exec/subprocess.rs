use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read};
use std::path::Path;
use std::process::{Command, ExitStatus, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

/// A line of subprocess output, tagged by the stream it arrived on.
#[derive(Debug)]
pub(crate) enum SubprocessLine {
    Stdout(String),
    Stderr(String),
}

/// Errors from spawning or supervising a subprocess.
#[derive(Debug)]
pub(crate) enum SubprocessError {
    /// The program could not be started at all. No command is stored: every
    /// caller wraps the error with its own command context.
    Spawn(std::io::Error),
    /// The program ran past its timeout and was killed; partial output is
    /// kept for diagnostics.
    Timeout {
        command: String,
        partial_stdout: String,
        partial_stderr: String,
    },
}

impl std::fmt::Display for SubprocessError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SubprocessError::Spawn(e) => write!(f, "failed to spawn process: {}", e),
            SubprocessError::Timeout {
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
                    write!(f, "command timed out: {}", command)
                } else {
                    write!(f, "command timed out: {}: {}", command, detail)
                }
            }
        }
    }
}

impl std::error::Error for SubprocessError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            SubprocessError::Spawn(e) => Some(e),
            _ => None,
        }
    }
}

/// Describe why a subprocess exited unsuccessfully: the signal that killed
/// it or its exit code. Single home for the signal-vs-code branch shared by
/// every exit-status check.
pub(crate) fn describe_exit_failure(status: &ExitStatus) -> String {
    use std::os::unix::process::ExitStatusExt;
    match (status.signal(), status.code()) {
        (Some(signal), _) => format!("terminated by signal {}", signal),
        (None, Some(code)) => format!("exited with code {}", code),
        (None, None) => "exited abnormally".to_string(),
    }
}

/// Spawn a reader thread that forwards every line of one child stream
/// through the channel until EOF.
fn spawn_stream_reader<T: Read + Send + 'static>(
    stream: Option<T>,
    tag: fn(String) -> SubprocessLine,
    sender: mpsc::Sender<SubprocessLine>,
) -> Option<thread::JoinHandle<()>> {
    stream.map(|stream| {
        thread::spawn(move || {
            for line in BufReader::new(stream).lines().map_while(Result::ok) {
                if sender.send(tag(line)).is_err() {
                    break;
                }
            }
        })
    })
}

/// Run a program with piped output, streaming its stdout/stderr lines
/// through `on_line` as they arrive. `cmd_desc` names the invocation in
/// timeout diagnostics; `argv`'s first element is the program. When
/// `timeout` is set the process is killed once it runs longer than that,
/// keeping the partial output in the error.
///
/// The exit status is returned to the caller, who decides whether non-zero
/// exit is an error (var-shell probes deliberately treat it as an empty
/// result). This is the single spawn/read/wait implementation shared by the
/// var-shell capture, `exec` statement streaming, and git sync paths.
pub(crate) fn run_subprocess(
    cmd_desc: &str,
    argv: &[&str],
    working_dir: Option<&Path>,
    env_overrides: Option<&HashMap<String, String>>,
    timeout: Option<Duration>,
    on_line: &mut dyn FnMut(SubprocessLine),
) -> Result<ExitStatus, SubprocessError> {
    let (program, args) = argv.split_first().ok_or_else(|| {
        SubprocessError::Spawn(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "run_subprocess requires at least one argument (argv[0])",
        ))
    })?;
    let mut command = Command::new(program);
    command
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(dir) = working_dir {
        command.current_dir(dir);
    }
    if let Some(overrides) = env_overrides {
        command.envs(overrides);
    }
    let mut child = command.spawn().map_err(SubprocessError::Spawn)?;

    // Each stream is drained by a reader thread forwarding lines through a
    // channel. The channel closes once every reader hits EOF, which marks
    // the moment the child's output is fully flushed.
    let (line_sender, line_receiver) = mpsc::channel::<SubprocessLine>();
    let _stdout_reader = spawn_stream_reader(
        child.stdout.take(),
        SubprocessLine::Stdout,
        line_sender.clone(),
    );
    let _stderr_reader = spawn_stream_reader(
        child.stderr.take(),
        SubprocessLine::Stderr,
        line_sender.clone(),
    );
    drop(line_sender);

    let start = Instant::now();
    let mut partial_stdout = String::new();
    let mut partial_stderr = String::new();

    loop {
        match line_receiver.recv_timeout(Duration::from_millis(50)) {
            Ok(SubprocessLine::Stdout(line)) => {
                partial_stdout.push_str(&line);
                partial_stdout.push('\n');
                on_line(SubprocessLine::Stdout(line));
            }
            Ok(SubprocessLine::Stderr(line)) => {
                partial_stderr.push_str(&line);
                partial_stderr.push('\n');
                on_line(SubprocessLine::Stderr(line));
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
            Err(mpsc::RecvTimeoutError::Timeout) => {
                if timeout.is_some_and(|limit| start.elapsed() >= limit) {
                    let _ = child.kill();
                    let _ = child.wait();
                    return Err(SubprocessError::Timeout {
                        command: cmd_desc.to_string(),
                        partial_stdout,
                        partial_stderr,
                    });
                }
            }
        }
    }

    child.wait().map_err(SubprocessError::Spawn)
}

/// Capture stdout of `argv`. Non-zero exit is tolerated (whatever stdout
/// was produced is returned). Returns `Err(Timeout { .. })` when the process
/// exceeds the optional timeout. Single capture implementation shared by the
/// runtime command capture and the compile-time import-path capture.
pub(crate) fn capture_argv(
    argv: &[&str],
    cmd_desc: &str,
    cwd: Option<&Path>,
    env: Option<&HashMap<String, String>>,
    timeout: Option<Duration>,
) -> Result<String, SubprocessError> {
    let mut captured = String::new();
    run_subprocess(cmd_desc, argv, cwd, env, timeout, &mut |line| match line {
        SubprocessLine::Stdout(text) => {
            captured.push_str(&text);
            captured.push('\n');
        }
        SubprocessLine::Stderr(_) => {}
    })?;
    Ok(captured.trim_end().to_string())
}
