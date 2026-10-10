//! Spawning, reaping, signalling, and sleeping on processes.
//!
//! `spawn` runs an argv in its own process group without a shell and returns
//! the pid; `wait_for` reaps it and returns the exit code; `signal` sends a
//! named signal to its group. A stop condition sends SIGTERM to every live
//! group, then SIGKILL after the grace period. A thread that does not own the
//! terminal runs with stdout and stderr discarded.

use std::os::unix::process::{CommandExt, ExitStatusExt};
use std::process::{Command as ProcessCommand, ExitStatus, Stdio};
use std::time::{Duration, Instant};

#[cfg(target_os = "linux")]
use nix::errno::Errno;
#[cfg(target_os = "linux")]
use nix::sys::prctl::set_pdeathsig;
use nix::sys::signal::Signal;
use nix::unistd::Pid;
#[cfg(target_os = "linux")]
use nix::unistd::getppid;

use crate::model::Record;
use crate::runtime::Panic;

use super::signals::signal_group;
use super::state::RuntimeState;

/// How long a stopping group is given before SIGKILL.
const STOP_GRACE: Duration = Duration::from_secs(2);

/// How often the waiting loop checks the child, cancellation, and the clock.
const POLL_INTERVAL: Duration = Duration::from_millis(20);

/// Spawn `argv` in its own process group without a shell and return its pid.
/// `spec` may name `Dir` (working directory) and `Stdin`/`Stdout`/`Stderr`
/// ("null" discards the stream; anything else inherits it).
pub(crate) fn spawn(state: &RuntimeState, spec: &Record, argv: &[String]) -> Result<i32, String> {
    let Some(program) = argv.first() else {
        return Err("`spawn` needs at least the program to run".to_owned());
    };
    let mut command = command_for(spec, argv);
    let mut child = command
        .spawn()
        .map_err(|error| format!("{program}: {error}"))?;
    let pid = child.id() as i32;
    if !state.register(pid) {
        signal_group(pid, Signal::SIGKILL);
        let _ = child.wait();
        return Err("too many concurrent processes".to_owned());
    }
    state.store_child(pid, child);
    Ok(pid)
}

/// Why a `wait_for` ended without a status.
pub(crate) enum WaitFailure {
    /// An OS or pid error, reported under the native's own name.
    Reported(String),
    /// The run is stopping; unwind without reporting.
    Cancelled,
}

/// Reap the process `pid` and return its exit code: a signal is `128 + signal`,
/// and an ordinary exit is its own code. A stop condition ends the wait early.
pub(crate) fn wait_for(state: &RuntimeState, pid: i32) -> Result<i32, WaitFailure> {
    let Some(mut child) = state.take_child(pid) else {
        return Err(WaitFailure::Reported(format!("{pid}: no such process")));
    };
    let waited = wait_for_stop(state, &mut child, pid);
    state.unregister(pid);
    let status = waited?;
    if state.cancelled() {
        return Err(WaitFailure::Cancelled);
    }
    Ok(exit_code(status))
}

/// Send a named signal to the process group `pid`.
pub(crate) fn signal(pid: i32, name: &str) -> Result<(), String> {
    let signal = parse_signal(name)?;
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

/// The signal a name selects.
fn parse_signal(name: &str) -> Result<Signal, String> {
    let signal = match name {
        "TERM" => Signal::SIGTERM,
        "KILL" => Signal::SIGKILL,
        "INT" => Signal::SIGINT,
        "HUP" => Signal::SIGHUP,
        "QUIT" => Signal::SIGQUIT,
        "USR1" => Signal::SIGUSR1,
        "USR2" => Signal::SIGUSR2,
        "STOP" => Signal::SIGSTOP,
        "CONT" => Signal::SIGCONT,
        other => return Err(format!("unknown signal `{other}`")),
    };
    Ok(signal)
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

/// The code of a finished command: a signal is `128 + signal`, and an
/// ordinary exit is its own code.
fn exit_code(status: ExitStatus) -> i32 {
    if let Some(signal) = status.signal() {
        return 128 + signal;
    }
    status.code().unwrap_or(1)
}

fn command_for(spec: &Record, argv: &[String]) -> ProcessCommand {
    let mut command = ProcessCommand::new(&argv[0]);
    command.args(&argv[1..]);
    let directory = spec.get("Dir");
    if !directory.is_empty() {
        command.current_dir(directory);
    }
    command.stdin(stdio_mode(spec.get("Stdin")));
    command.stdout(stdio_mode(spec.get("Stdout")));
    command.stderr(stdio_mode(spec.get("Stderr")));

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

/// The stdio a stream mode selects: `"null"` discards the stream, and anything
/// else inherits it.
fn stdio_mode(mode: &str) -> Stdio {
    if mode == "null" {
        Stdio::null()
    } else {
        Stdio::inherit()
    }
}

/// Report a runtime failure the way `std::eprint` does, then record it. The
/// failure stops the run.
pub(crate) fn failure(state: &RuntimeState, line: &str, reason: &str) -> Panic {
    state.fail_fast();
    crate::runtime::report_error(&format!("{line}: {reason}"));
    Panic
}
