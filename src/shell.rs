use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read};
use std::path::Path;
use std::process::{Command, ExitStatus, Stdio};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

/// The maximum time a `var shell` capture may run before it is killed.
const COMMAND_TIMEOUT: Duration = Duration::from_secs(30);

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
    let (program, args) = argv
        .split_first()
        .expect("run_subprocess requires the program as argv[0]");
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

/// Execute a shell command for a `var shell` statement.
///
/// Semantics match the shell's `$(...)`: a command that runs but exits
/// non-zero (e.g. a probe like `` `test -f x && echo yes` `` that fails its
/// condition) yields an empty string — this is the intended probe/boolean
/// idiom and must NOT be an error. Only a genuine environment failure is
/// fatal: a spawn failure (the shell could not be started at all) and a
/// timeout (the command hung) are surfaced to the caller as a
/// `SubprocessError`. The caller owns the variable's span, so it is
/// responsible for wrapping that error in a `CompileError` with source
/// position — `shell` stays free of any compile-time type.
///
/// This is the single funnel for every `var shell` command. There is no
/// memoization: each `var shell` is executed live at the point it is resolved
/// (globals during the linear pass, project/function vars during the resolve
/// pass), so the same command declared twice runs twice.
///
/// `working_dir` — when `Some`, runs the command in that directory;
/// when `None`, runs in the current process directory.
pub(crate) fn execute_shell_variable(
    resolved_command: &str,
    working_dir: Option<&Path>,
) -> Result<String, SubprocessError> {
    let mut captured_stdout = String::new();
    let shell_path = get_current_shell_path();
    let result = run_subprocess(
        resolved_command,
        &[&shell_path, "-c", resolved_command],
        working_dir,
        None,
        Some(COMMAND_TIMEOUT),
        &mut |line| {
            if let SubprocessLine::Stdout(line) = line {
                captured_stdout.push_str(&line);
                captured_stdout.push('\n');
            }
        },
    );
    match result {
        // Non-zero exit is not an error — empty string is the probe idiom.
        Ok(status) if !status.success() => Ok(String::new()),
        Ok(_) => Ok(captured_stdout.trim_end().to_string()),
        Err(e) => Err(e),
    }
}

/// Return the user's shell from `$SHELL`, defaulting to `"sh"`.
pub(crate) fn get_current_shell_path() -> String {
    std::env::var("SHELL").unwrap_or_else(|_| "sh".to_string())
}
