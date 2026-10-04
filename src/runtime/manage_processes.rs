//! Process execution: process groups, signals, and stopping.

use std::os::unix::process::{CommandExt, ExitStatusExt};
use std::process::{Command as ProcessCommand, ExitStatus, Stdio};
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicI32, AtomicPtr, Ordering};
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
use crate::compiler::MutexExt;

/// The most process groups one run can track at once. A run that exceeds it
/// fails rather than silently leaking an untracked group.
pub(crate) const MAX_GROUPS: usize = 1024;

/// The prefix every runtime diagnostic carries.
pub(super) const DIAGNOSTIC_PREFIX: &str = "kiru";

/// How long a stopping group is given before SIGKILL.
const STOP_GRACE: Duration = Duration::from_secs(2);

/// How often the waiting loop checks the child, cancellation, and the clock.
const POLL_INTERVAL: Duration = Duration::from_millis(20);

/// State shared by every body: whether the run is stopping, which signal
/// arrived, whether the run has failed, the process groups currently
/// running, and the threads `async` has started.
///
/// A `wait` joins the threads its own thread spawned; the run joins any
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

/// One thread `async` started, remembered together with the id of the
/// thread that spawned it. The pair lets `wait` join exactly the
/// caller's own children.
struct ChildThread {
    parent: ThreadId,
    join_handle: JoinHandle<()>,
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
        let join_handle = std::thread::spawn(body);
        self.threads.lock_unpoisoned().push(ChildThread {
            parent,
            join_handle,
        });
    }

    /// Join every thread `parent` spawned and leave every other entry in the
    /// table. The wait belongs to the spawning thread, so a spawned thread
    /// that waits joins only its own children and never its spawner or a
    /// sibling.
    pub(crate) fn join_children(&self, parent: ThreadId) {
        let children: Vec<JoinHandle<()>> = {
            let mut threads = self.threads.lock_unpoisoned();
            let mut children = Vec::new();
            let mut kept = Vec::with_capacity(threads.len());
            for child in std::mem::take(&mut *threads) {
                if child.parent == parent {
                    children.push(child.join_handle);
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
                let mut threads = self.threads.lock_unpoisoned();
                std::mem::take(&mut *threads)
                    .into_iter()
                    .map(|child| child.join_handle)
                    .collect()
            };
            if joins.is_empty() {
                return;
            }
            for join_handle in joins {
                let _ = join_handle.join();
            }
        }
    }

    pub(crate) fn cancelled(&self) -> bool {
        self.cancelled.load(Ordering::SeqCst)
    }

    pub(crate) fn interrupted(&self) -> i32 {
        self.interrupted.load(Ordering::SeqCst)
    }

    /// Whether the run has failed anywhere, through a `panic;` or a
    /// runtime failure. The exit code uses this after every thread joins.
    pub(crate) fn panicked(&self) -> bool {
        self.panicked.load(Ordering::SeqCst)
    }

    /// `panic;`: record that the run failed. A panic stops only the body
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

    /// Publish a group. The slot is claimed before the id is stored, and the
    /// id is zeroed before the slot is released, so a signal handler that sees
    /// a claimed slot reads either the current id or zero, never a recycled
    /// one.
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
pub(crate) fn install_signal_handlers(state: &RuntimeState) -> std::io::Result<()> {
    SIGNAL_HANDLER_TARGET.store(
        state as *const RuntimeState as *mut RuntimeState,
        Ordering::SeqCst,
    );
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
        // SAFETY: `install_signal_handlers` sets the pointer once to a leaked
        // `RuntimeState` that lives for the process lifetime and never clears
        // it. `on_signal` only performs atomic stores and `killpg`, so the
        // handler stays async-signal-safe.
        unsafe {
            (*pointer).on_signal(signal);
        }
    }
}

/// Run one command line through the POSIX shell and return its exit code. The
/// shell is fixed so that `std::quote`'s POSIX escaping always holds.
/// `output_muted` is true on a thread that does not own the terminal: stdout
/// and stderr are then discarded. Nothing is captured.
pub(crate) fn run(state: &RuntimeState, line: &str, output_muted: bool) -> Result<i32, Panic> {
    execute(
        state,
        CommandSpec {
            program: "/bin/sh",
            arguments: vec!["-c".to_owned(), line.to_owned()],
            output_muted,
            line,
        },
    )
}

/// One command about to run, with everything the kernel needs.
struct CommandSpec<'a> {
    program: &'a str,
    arguments: Vec<String>,
    output_muted: bool,
    /// The text shown when the command cannot run.
    line: &'a str,
}

/// Spawn, track, and wait for one command line in its own process group.
fn execute(state: &RuntimeState, spec: CommandSpec<'_>) -> Result<i32, Panic> {
    let mut command = command_for(&spec);
    let mut child = command
        .spawn()
        .map_err(|error| failure(state, spec.line, &error.to_string(), spec.output_muted))?;
    let pgid = child.id() as i32;
    if !state.register(pgid) {
        signal_group(pgid, Signal::SIGKILL);
        let _ = child.wait();
        return Err(failure(
            state,
            spec.line,
            "too many concurrent commands",
            spec.output_muted,
        ));
    }

    let waited = wait_for_stop(state, &mut child, pgid, spec.line, spec.output_muted);
    state.unregister(pgid);
    let status = waited?;

    if state.cancelled() {
        return Err(Panic);
    }

    Ok(exit_code(status))
}

/// Poll the child while watching for cancellation. A stop condition sends
/// SIGTERM, then SIGKILL after the grace period.
fn wait_for_stop(
    state: &RuntimeState,
    child: &mut std::process::Child,
    pgid: i32,
    line_for_errors: &str,
    output_muted: bool,
) -> Result<ExitStatus, Panic> {
    let mut terminating: Option<Instant> = None;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Ok(status),
            Ok(None) => {}
            Err(error) => {
                return Err(failure(
                    state,
                    line_for_errors,
                    &error.to_string(),
                    output_muted,
                ));
            }
        }

        let now = Instant::now();
        if terminating.is_none() {
            if state.cancelled() {
                signal_group(pgid, Signal::SIGTERM);
                terminating = Some(now);
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
    command.stdin(Stdio::inherit());
    if spec.output_muted {
        command.stdout(Stdio::null());
        command.stderr(Stdio::null());
    } else {
        command.stdout(Stdio::inherit());
        command.stderr(Stdio::inherit());
    }

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

/// Send `signal` to the process group `pgid`. A group whose last process has
/// already exited makes the call fail, which is not a runtime failure.
fn signal_group(pgid: i32, signal: Signal) {
    let _ = killpg(Pid::from_raw(pgid), signal);
}

/// Report a runtime failure and record it. The failure unwinds only the body
/// that hit it; the run still exits nonzero after every thread joins. An
/// output-muted thread does not own the terminal, so it prints nothing.
fn failure(state: &RuntimeState, line: &str, reason: &str, output_muted: bool) -> Panic {
    state.record_failure();
    if !output_muted {
        eprintln!("{DIAGNOSTIC_PREFIX}: {line}: {reason}");
    }
    Panic
}

/// Escape text as one POSIX shell word: wrap it in single quotes and replace
/// an embedded quote with the `'\''` sequence, so the shell reads exactly the
/// text and nothing else.
pub(crate) fn quote_shell_word(text: &str) -> String {
    let mut quoted = String::with_capacity(text.len() + 2);
    quoted.push('\'');
    for character in text.chars() {
        if character == '\'' {
            quoted.push_str("'\\''");
        } else {
            quoted.push(character);
        }
    }
    quoted.push('\'');
    quoted
}
