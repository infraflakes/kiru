//! Signal handling and the process-group table.
//!
//! A signal handler may only touch atomics and `killpg`, which are
//! async-signal-safe; escalation to SIGKILL happens in the waiting loop. The
//! group table is claimed before an id is stored and released after it is
//! zeroed, so a handler that sees a claimed slot reads either the current id or
//! zero, never a recycled one.

use std::sync::atomic::{AtomicPtr, Ordering};

use nix::sys::signal::{SaFlags, SigAction, SigHandler, SigSet, Signal, killpg, sigaction};
use nix::unistd::Pid;

use super::state::{MAX_GROUPS, RuntimeState};

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

impl RuntimeState {
    /// Called from a signal handler: only atomics and `killpg`, which are
    /// async-signal-safe. Escalation to SIGKILL happens in the waiting loop.
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

    /// Publish a group. The slot is claimed before the id is stored, and the
    /// id is zeroed before the slot is released, so a signal handler that sees
    /// a claimed slot reads either the current id or zero, never a recycled
    /// one.
    pub(super) fn register(&self, pgid: i32) -> bool {
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

    pub(super) fn unregister(&self, pgid: i32) {
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

/// Send `signal` to the process group `pgid`. A group whose last process has
/// already exited makes the call fail, which is not a runtime failure.
pub(super) fn signal_group(pgid: i32, signal: Signal) {
    let _ = killpg(Pid::from_raw(pgid), signal);
}
