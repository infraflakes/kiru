//! Command execution: process groups, timeouts, signals, and terminals.

use std::io::{ErrorKind, Read, Write};
use std::os::unix::process::{CommandExt, ExitStatusExt};
use std::process::{Command as ProcessCommand, ExitStatus, Stdio};
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicPtr, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::{JoinHandle, ThreadId};
use std::time::{Duration, Instant};

#[cfg(target_os = "linux")]
use nix::errno::Errno;
#[cfg(target_os = "linux")]
use nix::sys::prctl::set_pdeathsig;
use nix::sys::signal::{SaFlags, SigAction, SigHandler, SigSet, Signal, killpg, sigaction};
use nix::unistd::Pid;
#[cfg(target_os = "linux")]
use nix::unistd::getppid;

use super::Panic;
use crate::compiler::Command;

/// The most process groups one run can track at once. A run that exceeds it
/// fails rather than silently leaking an untracked group.
pub(crate) const MAX_GROUPS: usize = 1024;

/// How long a stopping group is given before SIGKILL.
const STOP_GRACE: Duration = Duration::from_secs(2);

/// How often the waiting loop checks the child, cancellation, and the clock.
const POLL_INTERVAL: Duration = Duration::from_millis(20);

/// State shared by every body: whether the run is stopping, which signal
/// arrived, whether the run has failed, the process groups currently
/// running, and the threads `std::async` has started.
///
/// A wait joins the threads its own thread spawned; the run joins any
/// remainder after the entry body ends.
pub(crate) struct RuntimeState {
    interrupted: AtomicI32,
    halted: AtomicBool,
    cancelled: AtomicBool,
    panicked: AtomicBool,
    group_ids: [AtomicI32; MAX_GROUPS],
    slots_in_use: [AtomicBool; MAX_GROUPS],
    threads: Mutex<Vec<ChildThread>>,
}

/// One thread `std::async` started, remembered together with the id of the
/// thread that spawned it. The pair lets `std::wait` join exactly the
/// caller's own children.
struct ChildThread {
    parent: ThreadId,
    join: JoinHandle<()>,
}

impl RuntimeState {
    pub(crate) fn new() -> Self {
        Self {
            interrupted: AtomicI32::new(0),
            halted: AtomicBool::new(false),
            cancelled: AtomicBool::new(false),
            panicked: AtomicBool::new(false),
            group_ids: [const { AtomicI32::new(0) }; MAX_GROUPS],
            slots_in_use: [const { AtomicBool::new(false) }; MAX_GROUPS],
            threads: Mutex::new(Vec::new()),
        }
    }

    /// Start `body` on a new OS thread and record the spawning thread as its
    /// parent. Nothing joins it until a wait on the parent or the end of the
    /// run.
    pub(crate) fn spawn_thread(&self, body: impl FnOnce() + Send + 'static) {
        let parent = std::thread::current().id();
        let join = std::thread::spawn(body);
        self.threads
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .push(ChildThread { parent, join });
    }

    /// Join every thread `parent` spawned and leave every other entry in the
    /// table. The wait belongs to the spawning thread, so a spawned thread
    /// that waits joins only its own children and never its spawner or a
    /// sibling.
    pub(crate) fn join_children(&self, parent: ThreadId) {
        let children: Vec<JoinHandle<()>> = {
            let mut threads = self
                .threads
                .lock()
                .unwrap_or_else(|poisoned| poisoned.into_inner());
            let mut children = Vec::new();
            let mut kept = Vec::with_capacity(threads.len());
            for child in std::mem::take(&mut *threads) {
                if child.parent == parent {
                    children.push(child.join);
                } else {
                    kept.push(child);
                }
            }
            *threads = kept;
            children
        };
        for child in children {
            let _ = child.join();
        }
    }

    /// Join every outstanding thread regardless of its parent. A joined
    /// thread may have spawned another, so the sweep repeats until the table
    /// holds no live thread. The end of the run calls this.
    pub(crate) fn join_all_threads(&self) {
        loop {
            let joins: Vec<JoinHandle<()>> = {
                let mut threads = self
                    .threads
                    .lock()
                    .unwrap_or_else(|poisoned| poisoned.into_inner());
                std::mem::take(&mut *threads)
                    .into_iter()
                    .map(|child| child.join)
                    .collect()
            };
            if joins.is_empty() {
                return;
            }
            for join in joins {
                let _ = join.join();
            }
        }
    }

    pub(crate) fn cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }

    pub(crate) fn interrupted(&self) -> i32 {
        self.interrupted.load(Ordering::SeqCst)
    }

    /// Whether the run has failed anywhere, through a `std::panic` or a
    /// runtime failure. The exit code uses this after every thread joins.
    pub(crate) fn panicked(&self) -> bool {
        self.panicked.load(Ordering::SeqCst)
    }

    /// `std::panic`: record that the run failed. A panic stops only the body
    /// that raised it; it never signals another body's process groups, so a
    /// panic in one thread leaves the others running.
    pub(crate) fn panic(&self) {
        self.record_failure();
    }

    /// Record that the run failed, so it exits nonzero once every thread has
    /// joined. Both a panic and a runtime failure go through here.
    pub(crate) fn record_failure(&self) {
        self.panicked.store(true, Ordering::SeqCst);
    }

    /// Let pending defers run their cleanup commands: a stopped run refuses
    /// new commands, so the stop is suspended while defers execute.
    pub(crate) fn begin_cleanup(&self) {
        self.cancelled.store(false, Ordering::SeqCst);
    }

    /// Cleanup is over; a halted run resumes refusing commands until it
    /// exits.
    pub(crate) fn end_cleanup(&self) {
        if self.halted.load(Ordering::SeqCst) {
            self.cancelled.store(true, Ordering::SeqCst);
        }
    }

    /// Called from a signal handler: only atomics and `killpg`, which are
    /// async-signal-safe. Escalation to SIGKILL happens in the waiting loop.
    fn on_signal(&self, signal: i32) {
        self.interrupted.store(signal, Ordering::SeqCst);
        self.halted.store(true, Ordering::SeqCst);
        self.cancelled.store(true, Ordering::SeqCst);
        self.signal_all(Signal::SIGTERM);
    }

    /// Publish a group. The id is stored before the slot is marked in use, so
    /// a signal handler can never read a stale id from a recycled slot.
    fn register(&self, pgid: i32) -> bool {
        for slot in 0..MAX_GROUPS {
            if self.slots_in_use[slot]
                .compare_exchange(false, true, Ordering::SeqCst, Ordering::SeqCst)
                .is_ok()
            {
                self.group_ids[slot].store(pgid, Ordering::SeqCst);
                return true;
            }
        }
        false
    }

    fn unregister(&self, pgid: i32) {
        for slot in 0..MAX_GROUPS {
            if self.slots_in_use[slot].load(Ordering::SeqCst)
                && self.group_ids[slot].load(Ordering::SeqCst) == pgid
            {
                self.group_ids[slot].store(0, Ordering::SeqCst);
                self.slots_in_use[slot].store(false, Ordering::SeqCst);
                return;
            }
        }
    }

    fn signal_all(&self, signal: Signal) {
        for slot in 0..MAX_GROUPS {
            if self.slots_in_use[slot].load(Ordering::SeqCst) {
                let pgid = self.group_ids[slot].load(Ordering::SeqCst);
                if pgid > 0 {
                    signal_group(pgid, signal);
                }
            }
        }
    }
}

static SIGNAL_HANDLER_TARGET: AtomicPtr<RuntimeState> = AtomicPtr::new(std::ptr::null_mut());

/// Install handlers for the interactive signals. The state must outlive the
/// process because the handler keeps a raw pointer to it.
pub(crate) fn install_signal_handlers(state: &Arc<RuntimeState>) -> std::io::Result<()> {
    SIGNAL_HANDLER_TARGET.store(Arc::as_ptr(state).cast_mut(), Ordering::SeqCst);
    let action = SigAction::new(
        SigHandler::Handler(handle_signal),
        SaFlags::empty(),
        SigSet::empty(),
    );
    for signal in [Signal::SIGINT, Signal::SIGTERM, Signal::SIGHUP] {
        // SAFETY: the action outlives this call, and the handler is a plain
        // `extern "C"` function that only touches atomics and `killpg`.
        unsafe {
            sigaction(signal, &action).map_err(std::io::Error::from)?;
        }
    }
    Ok(())
}

extern "C" fn handle_signal(signal: i32) {
    let pointer = SIGNAL_HANDLER_TARGET.load(Ordering::SeqCst);
    if !pointer.is_null() {
        unsafe {
            (*pointer).on_signal(signal);
        }
    }
}

/// What a finished command produced.
pub(crate) struct Outcome {
    pub(crate) code: i32,
    pub(crate) output: String,
}

/// Run one command. `capture` binds stdout, `forward` streams it live.
pub(crate) fn run(
    state: &RuntimeState,
    command: &Command,
    capture: bool,
    forward: bool,
    default_shell: Option<&str>,
) -> Result<Outcome, Panic> {
    let dir = command.dir.as_deref();

    if let Some(dir) = dir
        && dir.is_empty()
    {
        return Err(failure(state, &command.line, "dir is empty"));
    }
    if command.direnv && dir.is_none() {
        return Err(failure(state, &command.line, "direnv needs a dir"));
    }

    if command.direnv
        && let Some(dir) = dir
    {
        let allowed = execute(
            state,
            CommandSpec {
                program: "direnv",
                arguments: vec!["allow".to_owned(), dir.to_owned()],
                dir: Some(dir),
                env: &[],
                capture: false,
                forward: true,
                timeout: None,
                line: &command.line,
            },
        )?;
        if allowed.code != 0 {
            return Ok(Outcome {
                code: allowed.code,
                output: String::new(),
            });
        }
    }

    // An explicit `.shell(...)` is used verbatim, empty included; only when no
    // shell is set does the `$SHELL` default, then `sh`, apply.
    let shell = match command.shell.as_deref() {
        Some(shell) => shell,
        None => default_shell
            .filter(|shell| !shell.is_empty())
            .unwrap_or("sh"),
    };

    let (program, arguments) = if command.direnv && dir.is_some() {
        (
            "direnv",
            vec![
                "exec".to_owned(),
                dir.unwrap_or_default().to_owned(),
                shell.to_owned(),
                "-c".to_owned(),
                command.line.clone(),
            ],
        )
    } else {
        (shell, vec!["-c".to_owned(), command.line.clone()])
    };

    execute(
        state,
        CommandSpec {
            program,
            arguments,
            dir,
            env: command.env.entries(),
            capture,
            forward,
            timeout: command.timeout,
            line: &command.line,
        },
    )
}

/// One command about to run, with everything the kernel needs.
struct CommandSpec<'a> {
    program: &'a str,
    arguments: Vec<String>,
    dir: Option<&'a str>,
    env: &'a [(String, String)],
    capture: bool,
    forward: bool,
    timeout: Option<u64>,
    /// The text shown when the command cannot run.
    line: &'a str,
}

/// Spawn, track, and wait for one command line in its own process group.
fn execute(state: &RuntimeState, spec: CommandSpec<'_>) -> Result<Outcome, Panic> {
    let deadline = match spec.timeout.filter(|seconds| *seconds > 0) {
        Some(seconds) => match Instant::now().checked_add(Duration::from_secs(seconds)) {
            Some(deadline) => Some(deadline),
            None => return Err(failure(state, spec.line, "timeout is too large")),
        },
        None => None,
    };

    let mut command = command_for(&spec);
    let mut child = command
        .spawn()
        .map_err(|error| failure(state, spec.line, &error.to_string()))?;
    let pgid = child.id() as i32;
    if !state.register(pgid) {
        signal_group(pgid, Signal::SIGKILL);
        let _ = child.wait();
        return Err(failure(state, spec.line, "too many concurrent commands"));
    }

    let reader = if spec.capture {
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| failure(state, spec.line, "stdout is not piped for capture"))?;
        Some(std::thread::spawn(move || {
            read_stream(stdout, spec.forward)
        }))
    } else {
        None
    };

    let waited = wait_for_stop(state, &mut child, pgid, deadline, spec.line);
    state.unregister(pgid);
    let (status, timed_out) = waited?;

    let output = match reader {
        Some(handle) => handle.join().unwrap_or_default(),
        None => String::new(),
    };

    if state.cancelled() {
        return Err(Panic);
    }

    Ok(Outcome {
        code: if timed_out { 124 } else { exit_code(status) },
        output,
    })
}

/// Poll the child while watching for cancellation and the timeout deadline.
/// A stop condition sends SIGTERM, then SIGKILL after the grace period.
fn wait_for_stop(
    state: &RuntimeState,
    child: &mut std::process::Child,
    pgid: i32,
    deadline: Option<Instant>,
    line_for_errors: &str,
) -> Result<(ExitStatus, bool), Panic> {
    let mut terminating: Option<Instant> = None;
    let mut timed_out = false;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Ok((status, timed_out)),
            Ok(None) => {}
            Err(error) => return Err(failure(state, line_for_errors, &error.to_string())),
        }

        let now = Instant::now();
        if terminating.is_none() {
            if state.cancelled() {
                signal_group(pgid, Signal::SIGTERM);
                terminating = Some(now);
            } else if let Some(deadline) = deadline
                && now >= deadline
            {
                signal_group(pgid, Signal::SIGTERM);
                terminating = Some(now);
                timed_out = true;
            }
        } else if let Some(started) = terminating
            && now.duration_since(started) >= STOP_GRACE
        {
            signal_group(pgid, Signal::SIGKILL);
            terminating = Some(now);
        }
        std::thread::sleep(POLL_INTERVAL);
    }
}

/// The code of a finished command: a signal is `128 + signal`, and an
/// ordinary exit is its own code.
fn exit_code(status: ExitStatus) -> i32 {
    if let Some(signal) = status.signal() {
        return 128 + signal;
    }
    status.code().unwrap_or(1)
}

fn command_for(spec: &CommandSpec<'_>) -> ProcessCommand {
    let mut command = ProcessCommand::new(spec.program);
    command.args(&spec.arguments);
    if let Some(dir) = spec.dir {
        command.current_dir(dir);
    }
    for (key, value) in spec.env {
        command.env(key, value);
    }
    command.stdin(Stdio::inherit());
    command.stderr(Stdio::inherit());
    command.stdout(if spec.capture {
        Stdio::piped()
    } else if spec.forward {
        Stdio::inherit()
    } else {
        Stdio::null()
    });

    command.process_group(0);
    #[cfg(target_os = "linux")]
    // SAFETY: the closure runs between fork and exec, and only calls
    // async-signal-safe functions (prctl, getppid, and an errno error).
    unsafe {
        command.pre_exec(|| {
            set_pdeathsig(Signal::SIGKILL).map_err(std::io::Error::from)?;
            // The parent may have died between fork and prctl; in that case
            // the death signal was already delivered or never will be.
            if getppid() == Pid::from_raw(1) {
                return Err(std::io::Error::from(Errno::ESRCH));
            }
            Ok(())
        });
    }
    command
}

fn read_stream(mut stdout: impl Read, forward: bool) -> String {
    let mut bytes = Vec::new();
    let mut chunk = [0u8; 8192];
    loop {
        match stdout.read(&mut chunk) {
            Ok(0) => break,
            Ok(count) => {
                if forward {
                    let mut out = std::io::stdout();
                    let _ = out.write_all(&chunk[..count]);
                    let _ = out.flush();
                }
                bytes.extend_from_slice(&chunk[..count]);
            }
            Err(error) if error.kind() == ErrorKind::Interrupted => continue,
            Err(_) => break,
        }
    }
    String::from_utf8_lossy(&bytes)
        .trim_end_matches(['\n', '\r'])
        .to_owned()
}

/// Send `signal` to the process group `pgid`. A group whose last process has
/// already exited makes the call fail, which is not a runtime failure.
fn signal_group(pgid: i32, signal: Signal) {
    let _ = killpg(Pid::from_raw(pgid), signal);
}

/// Report a runtime failure and record it. The failure unwinds only the body
/// that hit it; the run still exits nonzero after every thread joins.
fn failure(state: &RuntimeState, line: &str, reason: &str) -> Panic {
    state.record_failure();
    eprintln!("kiru: {line}: {reason}");
    Panic
}
