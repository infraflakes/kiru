//! The run state shared by every body: lifecycle flags and the thread and
//! process tables.
//!
//! The signal flags live here, because a signal handler and the waiting loop
//! both read them; `signals` owns the handlers and the group-table operations,
//! and `threads` and `child_processes` own their tables.

use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};

use super::child_processes::ProcessTable;
use super::threads::ThreadTable;

/// State shared by every body: whether the run is stopping, which signal
/// arrived, whether the run has failed, the process groups currently
/// running, and the threads `async` has started.
///
/// A `wait` joins the threads its own thread spawned; the run joins any
/// remainder after the entry body ends.
pub(crate) struct RuntimeState {
    pub(super) interrupted: AtomicI32,
    pub(super) cancelled: AtomicBool,
    panicked: AtomicBool,
    pub(crate) threads: ThreadTable,
    pub(crate) processes: ProcessTable,
}

impl RuntimeState {
    pub(crate) fn new() -> Self {
        Self {
            interrupted: AtomicI32::new(0),
            cancelled: AtomicBool::new(false),
            panicked: AtomicBool::new(false),
            threads: ThreadTable::new(),
            processes: ProcessTable::new(),
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

    /// `panic;`: fail the run on the spot, stopping every body and process
    /// group.
    pub(crate) fn panic(&self) {
        self.fail_fast();
    }

    /// Record that the run failed, so it exits nonzero once every thread has
    /// joined. Both a panic and a runtime failure go through here.
    pub(crate) fn record_failure(&self) {
        self.panicked.store(true, Ordering::SeqCst);
    }
}
