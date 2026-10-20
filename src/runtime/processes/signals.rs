//! Signal handling: a `signalfd` monitor thread, not an asynchronous handler.
//!
//! The interactive signals are blocked before the run starts, so they stay
//! pending instead of being delivered to a handler. A dedicated monitor thread
//! reads them from a `signalfd` and, on a normal thread, records the stop and
//! signals every live process group. That makes the whole signal path
//! async-signal-safe by construction: no handler runs, so the group table is an
//! ordinary mutex-guarded Vec and needs no fixed size or lock-free claim
//! protocol.
//!
//! This is the pattern the Linux `signalfd(2)` manual describes: a signal must
//! be blocked in every thread, otherwise it is delivered to a handler and is
//! not visible through the descriptor. Threads spawned by the run inherit the
//! blocked mask from the thread that started them.

use std::sync::Arc;
use std::sync::atomic::Ordering;

use nix::errno::Errno;
use nix::sys::signal::{SigSet, SigmaskHow, Signal, killpg, pthread_sigmask};
use nix::sys::signalfd::{SfdFlags, SignalFd};
use nix::unistd::Pid;

use super::state::RuntimeState;

/// The signals that stop a run.
pub(super) const STOP_SIGNALS: [Signal; 3] = [Signal::SIGINT, Signal::SIGTERM, Signal::SIGHUP];

/// Block the stop signals and start the monitor thread that reads them.
///
/// The caller must run on the thread that starts every other thread of the
/// program, so they all inherit the blocked mask.
pub(crate) fn install_signal_handlers(state: Arc<RuntimeState>) -> std::io::Result<()> {
    let mut mask = SigSet::empty();
    for signal in STOP_SIGNALS {
        mask.add(signal);
    }
    // Blocking the set is the process's own choice, and every thread the run
    // starts inherits the mask.
    pthread_sigmask(SigmaskHow::SIG_BLOCK, Some(&mask), None).map_err(std::io::Error::from)?;
    let signals =
        SignalFd::with_flags(&mask, SfdFlags::SFD_CLOEXEC).map_err(std::io::Error::from)?;
    std::thread::spawn(move || monitor(state, signals));
    Ok(())
}

/// Read stop signals for the life of the process. The thread owns both the
/// state and the descriptor, so the state stays alive as long as a signal may
/// arrive.
fn monitor(state: Arc<RuntimeState>, signals: SignalFd) {
    loop {
        match signals.read_signal() {
            Ok(Some(info)) => state.on_signal(info.ssi_signo as i32),
            Ok(None) | Err(Errno::EINTR) => {}
            Err(_) => return,
        }
    }
}

impl RuntimeState {
    /// Record a stop and signal every live process group. Only a normal thread
    /// calls this, so it may lock the group table and run `killpg`.
    fn on_signal(&self, signal: i32) {
        self.interrupted.store(signal, Ordering::SeqCst);
        self.cancelled.store(true, Ordering::SeqCst);
        self.signal_all(Signal::SIGTERM);
    }

    /// Fail the run on the spot: record the failure, stop every body, and
    /// signal every process group. A `panic;`, a runtime failure, and a failed
    /// thread all go through here.
    pub(crate) fn fail_fast(&self) {
        self.record_failure();
        self.cancelled.store(true, Ordering::SeqCst);
        self.signal_all(Signal::SIGTERM);
    }

    /// Send `signal` to every live process group.
    fn signal_all(&self, signal: Signal) {
        for pgid in self.processes.live_groups() {
            if pgid > 0 {
                signal_group(pgid, signal);
            }
        }
    }
}

/// Send `signal` to the process group `pgid`. A group whose last process has
/// already exited makes the call fail, which is not a runtime failure.
pub(super) fn signal_group(pgid: i32, signal: Signal) {
    let _ = killpg(Pid::from_raw(pgid), signal);
}
