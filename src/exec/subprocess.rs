use std::collections::{HashMap, HashSet};
use std::io::{BufRead, BufReader, Read};
use std::path::Path;
use std::process::{Command, ExitStatus, Stdio};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};

/// Tracks the live child process groups of one run and kills them as a set.
///
/// Every spawned command runs as its own process-group leader, so killing a
/// group takes down the command's entire tree: wrappers (e.g. `direnv exec`)
/// and grandchildren forked by the command itself. The kill switch is what
/// makes a run stoppable as a whole: one failing chain fails the run, and
/// keyboard-cancel must leave nothing running behind it.
///
/// This is the standard Unix process-tree kill: create each command as its
/// own group (`setpgid`) and signal the group with a negative pid. The
/// alternatives are strictly worse for this use: walking the process tree
/// misses grandchildren that re-parented after their parent exited (the
/// classic orphan bug), and PID namespaces are Linux-only and need
/// namespace tooling kiru should not require.
pub(crate) struct RunKillSwitch {
    /// Set once any chain of the run has failed. Steps check it before
    /// spawning so no new command starts after the run is already lost.
    failed: AtomicBool,
    /// Process-group ids of all live commands spawned under this switch.
    groups: Mutex<HashSet<i32>>,
    /// Process-group ids this switch has killed (fail-fast stop or cancel).
    /// A child that dies by signal can check this to know it was a victim
    /// of the run's stop rather than dying on its own.
    killed: Mutex<HashSet<i32>>,
}

impl RunKillSwitch {
    pub(crate) fn new() -> Self {
        RunKillSwitch {
            failed: AtomicBool::new(false),
            groups: Mutex::new(HashSet::new()),
            killed: Mutex::new(HashSet::new()),
        }
    }

    /// Mark the run as failed and kill every live command group.
    pub(crate) fn fail(&self) {
        self.failed.store(true, Ordering::SeqCst);
        self.kill_all();
    }

    /// Whether the run has already failed; no new step should spawn.
    pub(crate) fn is_failed(&self) -> bool {
        self.failed.load(Ordering::SeqCst)
    }

    /// Kill every live command group. The set is snapshotted under the lock
    /// and killed without it, so concurrent registration is never blocked by
    /// a syscall. Killed groups are recorded so their tasks can be told
    /// apart from tasks that died on their own.
    pub(crate) fn kill_all(&self) {
        let targets: Vec<i32> = self
            .groups
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .copied()
            .collect();
        for pgid in targets {
            self.killed
                .lock()
                .unwrap_or_else(|e| e.into_inner())
                .insert(pgid);
            kill_group(pgid);
        }
    }

    /// Whether this switch was what killed the group `pgid`.
    pub(crate) fn killed(&self, pgid: i32) -> bool {
        self.killed
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .contains(&pgid)
    }

    fn register(&self, pgid: i32) {
        self.groups
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(pgid);
    }

    fn deregister(&self, pgid: i32) {
        self.groups
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .remove(&pgid);
    }

    /// Number of live groups currently registered. Test diagnostic for the
    /// registration window between spawning a child and killing a run.
    #[cfg(test)]
    pub(crate) fn live_group_count(&self) -> usize {
        self.groups.lock().unwrap_or_else(|e| e.into_inner()).len()
    }
}

/// SIGKILL every process in the group `pgid`, leader included. The group
/// dies atomically from the kernel's view; there is no window in which a
/// child of the command survives its parent.
fn kill_group(pgid: i32) {
    // SAFETY: kill is the POSIX syscall taking two integers; it cannot
    // violate memory safety. ESRCH (group already gone) is ignored.
    let _ = unsafe { libc::kill(-pgid, libc::SIGKILL) };
}

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

/// The outcome of a finished child: its exit status plus whether the run's
/// kill switch was what terminated it — as opposed to the child exiting on
/// its own or being killed from outside the run.
#[derive(Debug)]
pub(crate) struct SubprocessExit {
    pub(crate) status: ExitStatus,
    pub(crate) killed_by_switch: bool,
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
    kill: Option<&RunKillSwitch>,
    on_line: &mut dyn FnMut(SubprocessLine),
) -> Result<SubprocessExit, SubprocessError> {
    use std::os::unix::process::CommandExt;

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
        .stderr(Stdio::piped())
        // Each command becomes its own process-group leader so a group kill
        // reaches its entire tree (wrappers and forked grandchildren) without
        // touching kiru's own group.
        .process_group(0);
    if let Some(dir) = working_dir {
        command.current_dir(dir);
    }
    if let Some(overrides) = env_overrides {
        command.envs(overrides);
    }
    let mut child = command.spawn().map_err(SubprocessError::Spawn)?;

    // The child leads its own group; `pgid == child.pid`. Registered so an
    // external stop (failing chain, keyboard cancel) can kill it as a set.
    let pgid = child.id() as i32;
    if let Some(kill) = kill {
        kill.register(pgid);
    }

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
                    // Kill the whole group: the direct child may itself have
                    // forked grandchildren that must not outlive the timeout.
                    kill_group(pgid);
                    let _ = child.wait();
                    if let Some(kill) = kill {
                        kill.deregister(pgid);
                    }
                    return Err(SubprocessError::Timeout {
                        command: cmd_desc.to_string(),
                        partial_stdout,
                        partial_stderr,
                    });
                }
            }
        }
    }

    let status = child.wait().map_err(SubprocessError::Spawn)?;
    if let Some(kill) = kill {
        kill.deregister(pgid);
    }
    // A child that died by signal while this switch also killed its group
    // was a victim of the run's stop, not an independent failure.
    let killed_by_switch = {
        use std::os::unix::process::ExitStatusExt;
        status.signal().is_some() && kill.is_some_and(|k| k.killed(pgid))
    };
    Ok(SubprocessExit {
        status,
        killed_by_switch,
    })
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
    kill: Option<&RunKillSwitch>,
) -> Result<String, SubprocessError> {
    let mut captured = String::new();
    run_subprocess(
        cmd_desc,
        argv,
        cwd,
        env,
        timeout,
        kill,
        &mut |line| match line {
            SubprocessLine::Stdout(text) => {
                captured.push_str(&text);
                captured.push('\n');
            }
            SubprocessLine::Stderr(_) => {}
        },
    )?;
    Ok(captured.trim_end().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn kill_switch_flag_transitions() {
        let switch = RunKillSwitch::new();
        assert!(!switch.is_failed(), "a fresh switch is not failed");
        // Killing with no live groups registered is a no-op.
        switch.kill_all();
        assert!(!switch.is_failed(), "kill_all alone does not mark failed");
        switch.fail();
        assert!(switch.is_failed(), "fail() marks the switch");
    }

    #[test]
    fn fail_kills_the_live_child_and_marks_it_as_a_victim() {
        let switch = Arc::new(RunKillSwitch::new());
        let switch_in_worker = Arc::clone(&switch);

        // A long-running child in its own thread, supervised by the switch.
        let worker = thread::spawn(move || {
            run_subprocess(
                "sleep 60",
                &["sleep", "60"],
                None,
                None,
                None,
                Some(switch_in_worker.as_ref()),
                &mut |_| {},
            )
        });

        // Wait for the child to spawn and register its group, so the kill
        // below exercises the real registry instead of racing an empty set.
        let registered = (0..40).find(|_| {
            thread::sleep(Duration::from_millis(50));
            switch.live_group_count() == 1
        });
        assert!(registered.is_some(), "child registered its group in time");

        switch.fail();
        let exit = worker
            .join()
            .unwrap()
            .expect("child was killed, not errored");

        // The switch killed the group: the child died by SIGKILL and is
        // recorded as a victim (killed_by_switch already proves the switch
        // recorded the kill), and the group left the live registry.
        use std::os::unix::process::ExitStatusExt;
        assert_eq!(exit.status.signal(), Some(libc::SIGKILL));
        assert!(exit.killed_by_switch, "killed child is a switch victim");
        assert_eq!(switch.live_group_count(), 0, "group deregistered");
    }
}
