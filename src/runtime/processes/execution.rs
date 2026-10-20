//! Spawning, reaping, signalling, and sleeping on processes.
//!
//! `spawn` runs an argv in its own process group without a shell and returns
//! the pid; `wait_for` reaps it and returns the raw `wait` status, which the
//! standard library decodes into an exit code; `signal` sends a numbered signal
//! to its group. A stop condition sends SIGTERM to every live group, then
//! SIGKILL after the grace period. A thread that does not own the terminal runs
//! with stdout and stderr discarded.

use std::os::unix::process::{CommandExt, ExitStatusExt};
use std::process::{Command as ProcessCommand, ExitStatus};
use std::time::{Duration, Instant};

#[cfg(target_os = "linux")]
use nix::errno::Errno;
#[cfg(target_os = "linux")]
use nix::sys::prctl::set_pdeathsig;
use nix::sys::signal::{SigSet, SigmaskHow, Signal, sigprocmask};
use nix::unistd::Pid;
#[cfg(target_os = "linux")]
use nix::unistd::getppid;

use crate::runtime::Panic;

use super::signals::{STOP_SIGNALS, signal_group};
use super::state::RuntimeState;

/// How long a stopping group is given before SIGKILL.
const STOP_GRACE: Duration = Duration::from_secs(2);

/// How often the waiting loop checks the child, cancellation, and the clock.
const POLL_INTERVAL: Duration = Duration::from_millis(20);

/// Spawn `argv` in its own process group without a shell and return its pid.
/// The child inherits every stream; a caller that wants another stream, a
/// directory, or an environment builds a shell line instead, so the syscall
/// stays the syscall.
pub(crate) fn spawn(state: &RuntimeState, argv: &[String]) -> Result<i32, String> {
    let Some(program) = argv.first() else {
        return Err("`spawn` needs at least the program to run".to_owned());
    };
    let mut command = command_for(argv);
    let child = command
        .spawn()
        .map_err(|error| format!("{program}: {error}"))?;
    let pid = child.id() as i32;
    state.processes.register(pid);
    state.processes.store_child(pid, child);
    Ok(pid)
}

/// Why a `wait_for` ended without a status.
pub(crate) enum WaitFailure {
    /// An OS or pid error, reported under the native's own name.
    Reported(String),
    /// The run is stopping; unwind without reporting.
    Cancelled,
}

/// Reap the process `pid` and return the kernel's raw `wait` status, untouched.
/// What the status means — `128 + signal` when a signal ended the process, its
/// own code otherwise — is the standard library's rule, decoded in
/// `std::process::wait_for`; the runtime forwards the word and owns no second
/// encoding. A stop condition ends the wait early.
pub(crate) fn wait_for(state: &RuntimeState, pid: i32) -> Result<i32, WaitFailure> {
    let Some(mut child) = state.processes.take_child(pid) else {
        return Err(WaitFailure::Reported(format!("{pid}: no such process")));
    };
    let waited = wait_for_stop(state, &mut child, pid);
    state.processes.unregister(pid);
    let status = waited?;
    if state.cancelled() {
        return Err(WaitFailure::Cancelled);
    }
    Ok(status.into_raw())
}

/// Send the signal `number` to the process group `pid`. The number is the
/// signal's ABI value; the name-to-number table lives in the standard library.
pub(crate) fn signal(pid: i32, number: i32) -> Result<(), String> {
    let signal =
        Signal::try_from(number).map_err(|_| format!("unknown signal number `{number}`"))?;
    signal_group(pid, signal);
    Ok(())
}

/// Sleep for `seconds` whole seconds, waking early when the run is cancelled
/// so a stop stays responsive during a long wait.
pub(crate) fn sleep_seconds(state: &RuntimeState, seconds: u64) -> Result<(), Panic> {
    let mut remaining = Duration::from_secs(seconds);
    while !remaining.is_zero() {
        if state.cancelled() {
            return Err(Panic);
        }
        let interval = remaining.min(POLL_INTERVAL);
        std::thread::sleep(interval);
        remaining = remaining.saturating_sub(interval);
    }
    if state.cancelled() {
        return Err(Panic);
    }
    Ok(())
}

/// Poll the child while watching for cancellation. A stop condition sends
/// SIGTERM, then SIGKILL after the grace period.
fn wait_for_stop(
    state: &RuntimeState,
    child: &mut std::process::Child,
    pgid: i32,
) -> Result<ExitStatus, WaitFailure> {
    let mut terminating: Option<Instant> = None;
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Ok(status),
            Ok(None) => {}
            Err(error) => return Err(WaitFailure::Reported(error.to_string())),
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

fn command_for(argv: &[String]) -> ProcessCommand {
    let mut command = ProcessCommand::new(&argv[0]);
    command.args(&argv[1..]);

    command.process_group(0);
    #[cfg(target_os = "linux")]
    // SAFETY: the closure runs between fork and exec, and only calls
    // async-signal-safe functions (`sigprocmask`, prctl, getppid, and an errno
    // error).
    unsafe {
        command.pre_exec(|| {
            // The run blocks the stop signals so its monitor can read them from
            // a signalfd; the child must not inherit that mask, or a stop would
            // leave the signal pending instead of ending it. `signalfd(2)`
            // documents unblocking before `execve` for exactly this case.
            let mut stop = SigSet::empty();
            for signal in STOP_SIGNALS {
                stop.add(signal);
            }
            sigprocmask(SigmaskHow::SIG_UNBLOCK, Some(&stop), None)
                .map_err(std::io::Error::from)?;
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

/// Report a runtime failure the way `std::eprint` does, then record it. The
/// failure stops the run.
pub(crate) fn failure(state: &RuntimeState, line: &str, reason: &str) -> Panic {
    state.fail_fast();
    crate::runtime::report_error(&format!("{line}: {reason}"));
    Panic
}
