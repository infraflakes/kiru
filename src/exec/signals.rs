//! Process-wide shutdown signals.
//!
//! A terminal `q`/Ctrl+C is not the only way a run ends: `kill`, a closed
//! terminal, or a non-TTY stdin all send SIGINT/SIGTERM/SIGHUP directly.
//! The handlers set one flag; the TUI loop observes it and runs the same
//! cancel path (kill every child group, exit non-zero). Only the atomic
//! store happens in the handler, so it is async-signal-safe.

use std::sync::atomic::{AtomicBool, Ordering};

/// Set by the signal handler, observed by the TUI loop.
static CANCEL_REQUESTED: AtomicBool = AtomicBool::new(false);
/// Handlers are installed once per process.
static HANDLERS_INSTALLED: AtomicBool = AtomicBool::new(false);

/// The signal handler: it only records that shutdown was requested.
extern "C" fn request_cancel(_signal: libc::c_int) {
    CANCEL_REQUESTED.store(true, Ordering::SeqCst);
}

/// Install the shutdown handlers once. Safe to call from every run.
pub(crate) fn install_shutdown_handlers() {
    if HANDLERS_INSTALLED.swap(true, Ordering::SeqCst) {
        return;
    }
    // SAFETY: the handler only performs an atomic store, and the sigaction
    // struct is fully initialized before it is registered.
    unsafe {
        let mut action: libc::sigaction = std::mem::zeroed();
        action.sa_sigaction = request_cancel as *const () as libc::sighandler_t;
        action.sa_flags = libc::SA_RESTART;
        for signal in [libc::SIGINT, libc::SIGTERM, libc::SIGHUP] {
            libc::sigaction(signal, &action, std::ptr::null_mut());
        }
    }
}

/// Whether a shutdown signal has been received.
pub(crate) fn cancel_requested() -> bool {
    CANCEL_REQUESTED.load(Ordering::SeqCst)
}
