use std::collections::{HashMap, HashSet};
use std::io::{BufRead, BufReader, Read};
use std::path::Path;
use std::process::{Command, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc;
use std::sync::{Arc, Mutex};
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
/// How long a group gets to exit on SIGTERM before it is SIGKILLed.
pub(crate) const GRACE_PERIOD: Duration = Duration::from_secs(2);

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

    /// Mark the run as failed and stop every live command group.
    pub(crate) fn fail(self: &Arc<Self>) {
        self.failed.store(true, Ordering::SeqCst);
        self.kill_all();
    }

    /// Whether the run has already failed; no new step should spawn.
    pub(crate) fn is_failed(&self) -> bool {
        self.failed.load(Ordering::SeqCst)
    }

    /// Stop every live command group: SIGTERM first so processes can clean
    /// up, then SIGKILL after a grace period for whatever is still
    /// registered. The set is snapshotted under the lock and signalled
    /// without it, so concurrent registration is never blocked by a syscall.
    /// Stopped groups are recorded so their tasks can be told apart from
    /// tasks that died on their own.
    pub(crate) fn kill_all(self: &Arc<Self>) {
        let targets: Vec<i32> = self
            .groups
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .copied()
            .collect();
        if targets.is_empty() {
            return;
        }
        for pgid in &targets {
            self.mark_killed(*pgid);
            signal_group(*pgid, libc::SIGTERM);
        }
        let switch = Arc::clone(self);
        thread::spawn(move || {
            thread::sleep(GRACE_PERIOD);
            for pgid in targets {
                let still_live = switch
                    .groups
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .contains(&pgid);
                if still_live {
                    signal_group(pgid, libc::SIGKILL);
                }
            }
        });
    }

    /// SIGKILL every group still registered, without waiting for the grace
    /// period. The cancel path hands off here because it exits immediately
    /// after, which would otherwise abandon the escalation thread.
    pub(crate) fn kill_survivors(&self) {
        let targets: Vec<i32> = self
            .groups
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .iter()
            .copied()
            .collect();
        for pgid in targets {
            signal_group(pgid, libc::SIGKILL);
        }
    }

    /// Record `pgid` as killed by this switch, so its task can distinguish
    /// an external stop from an independent death.
    pub(crate) fn mark_killed(&self, pgid: i32) {
        self.killed
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(pgid);
    }

    /// Stop one registered group synchronously: SIGTERM, wait up to the
    /// grace period for the leader to exit, then SIGKILL.
    pub(crate) fn terminate_group(&self, child: &mut std::process::Child, pgid: i32) {
        self.mark_killed(pgid);
        terminate_group(child, pgid);
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

/// Signal every process in the group `pgid`, leader included. The group is
/// addressed as a set from the kernel's view; there is no window in which a
/// child of the command survives its parent.
fn signal_group(pgid: i32, signal: i32) {
    // SAFETY: kill is the POSIX syscall taking two integers; it cannot
    // violate memory safety. ESRCH (group already gone) is ignored.
    let _ = unsafe { libc::kill(-pgid, signal) };
}

/// Stop one command group: SIGTERM so the command can clean up, a bounded
/// wait for its leader to exit, then SIGKILL for the whole group.
fn terminate_group(child: &mut std::process::Child, pgid: i32) {
    signal_group(pgid, libc::SIGTERM);
    let deadline = Instant::now() + GRACE_PERIOD;
    while Instant::now() < deadline {
        match child.try_wait() {
            Ok(Some(_)) => return,
            Ok(None) => thread::sleep(Duration::from_millis(10)),
            Err(_) => break,
        }
    }
    signal_group(pgid, libc::SIGKILL);
}

/// A line of subprocess output, tagged by the stream it arrived on.
#[derive(Debug, PartialEq, Eq)]
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
            let mut reader = BufReader::new(stream);
            loop {
                let mut bytes = Vec::new();
                match reader.read_until(b'\n', &mut bytes) {
                    Ok(0) => break,
                    Ok(_) => {
                        // Decode lossily: a single invalid byte must not
                        // truncate the rest of the stream. Trailing line
                        // endings are stripped, matching line semantics.
                        while matches!(bytes.last(), Some(b'\n' | b'\r')) {
                            bytes.pop();
                        }
                        let line = String::from_utf8_lossy(&bytes).into_owned();
                        if sender.send(tag(line)).is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
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
    #[cfg(target_os = "linux")]
    unsafe {
        // If kiru dies without stopping its children, the kernel signals
        // this child. The parent-pid recheck closes the race where kiru died
        // between fork and prctl.
        command.pre_exec(|| {
            let parent = libc::getppid();
            libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM);
            if libc::getppid() != parent {
                libc::raise(libc::SIGTERM);
            }
            Ok(())
        });
    }
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
        // The run can be lost between the caller's pre-spawn check and this
        // registration; a stop that missed this group must still not leave
        // it running.
        if kill.is_failed() {
            kill.terminate_group(&mut child, pgid);
            let status = child.wait().map_err(SubprocessError::Spawn)?;
            kill.deregister(pgid);
            return Ok(SubprocessExit {
                status,
                killed_by_switch: true,
            });
        }
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
    // Partial output exists only to describe a timeout; without one the
    // streams are forwarded untouched instead of duplicated into memory.
    let keep_partial = timeout.is_some();
    let mut partial_stdout = String::new();
    let mut partial_stderr = String::new();

    loop {
        let received = match timeout {
            // No time limit: block until a line arrives or every reader hit
            // EOF and closed the channel (the only `RecvError` cause). No
            // polling.
            None => line_receiver
                .recv()
                .map_err(|_| mpsc::RecvTimeoutError::Disconnected),
            // Wait at most the remaining budget, so a silent command times
            // out exactly when its deadline passes.
            Some(limit) => line_receiver.recv_timeout(
                limit
                    .saturating_sub(start.elapsed())
                    .max(Duration::from_millis(1)),
            ),
        };
        match received {
            Ok(SubprocessLine::Stdout(text)) => {
                if keep_partial {
                    partial_stdout.push_str(&text);
                    partial_stdout.push('\n');
                }
                on_line(SubprocessLine::Stdout(text));
            }
            Ok(SubprocessLine::Stderr(text)) => {
                if keep_partial {
                    partial_stderr.push_str(&text);
                    partial_stderr.push('\n');
                }
                on_line(SubprocessLine::Stderr(text));
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => break,
            Err(mpsc::RecvTimeoutError::Timeout) => {
                // Stop the whole group: the direct child may itself have
                // forked grandchildren that must not outlive the timeout.
                terminate_group(&mut child, pgid);
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

/// Capture stdout of `argv` as reconstructed text: one line per line break,
/// nothing trimmed. Non-zero exit is tolerated (whatever stdout was produced
/// is returned). Returns `Err(Timeout { .. })` when the process exceeds the
/// optional timeout. Single capture implementation shared by the runtime
/// command capture and the compile-time import-path capture; each caller
/// decides what trailing text its use allows.
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
    Ok(captured)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;

    #[test]
    fn kill_switch_flag_transitions() {
        let switch = Arc::new(RunKillSwitch::new());
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
        assert!(
            matches!(exit.status.signal(), Some(libc::SIGTERM | libc::SIGKILL)),
            "stopped by the switch's SIGTERM (or its SIGKILL escalation)"
        );
        assert!(exit.killed_by_switch, "killed child is a switch victim");
        assert_eq!(switch.live_group_count(), 0, "group deregistered");
    }

    /// Invalid UTF-8 must not truncate the stream: the rest of the output
    /// still arrives, with the bad byte decoded lossily.
    #[test]
    fn invalid_utf8_output_is_preserved_lossily() {
        let mut lines = Vec::new();
        run_subprocess(
            "printf",
            &["sh", "-c", "printf 'ok\n'; printf '\\377\n'"],
            None,
            None,
            None,
            None,
            &mut |line| lines.push(line),
        )
        .unwrap();
        assert_eq!(lines.len(), 2, "both lines are read: {lines:?}");
        assert_eq!(lines[0], SubprocessLine::Stdout("ok".to_string()));
        match &lines[1] {
            SubprocessLine::Stdout(line) => {
                assert!(line.contains('\u{FFFD}'), "lossy decode: {line:?}");
            }
            other => panic!("expected stdout, got {other:?}"),
        }
    }

    /// The capture primitive returns reconstructed text untrimmed; each
    /// caller decides what trailing text its use allows.
    #[test]
    fn capture_returns_text_for_the_caller_to_trim() {
        let text = capture_argv(
            &["sh", "-c", "printf 'x\\n\\n'"],
            "printf",
            None,
            None,
            None,
            None,
        )
        .unwrap();
        assert_eq!(text, "x\n\n");
    }

    /// A command that ignores SIGTERM is still stopped by the escalation.
    #[test]
    fn timeout_escalates_to_sigkill() {
        let start = Instant::now();
        let result = run_subprocess(
            "trap-term",
            &["sh", "-c", "trap '' TERM; sleep 60"],
            None,
            None,
            Some(Duration::from_millis(200)),
            None,
            &mut |_| {},
        );
        assert!(matches!(result, Err(SubprocessError::Timeout { .. })));
        assert!(
            start.elapsed() < Duration::from_secs(15),
            "escalation is bounded"
        );
    }
}
