//! The process table: the live process groups and the spawned children.
//!
//! A spawned process is claimed in the group table before its id is stored and
//! released after it is zeroed, so a signal handler that sees a claimed slot
//! reads either the current id or zero, never a recycled one. The child itself
//! is kept so `wait_for` can reap it by pid.

use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicI32};

use crate::lock_recovery::MutexExt;

/// The most process groups one run can track at once. A run that exceeds it
/// fails rather than silently leaking an untracked group.
pub(super) const MAX_GROUPS: usize = 1024;

/// The process groups currently running, and the spawned processes waiting to
/// be reaped by `wait_for`.
pub(crate) struct ProcessTable {
    pub(super) group_ids: [AtomicI32; MAX_GROUPS],
    pub(super) slots_in_use: [AtomicBool; MAX_GROUPS],
    /// Spawned processes waiting to be reaped by `wait_for`, keyed by pid.
    children: Mutex<HashMap<i32, std::process::Child>>,
}

impl ProcessTable {
    pub(crate) fn new() -> Self {
        Self {
            group_ids: [const { AtomicI32::new(0) }; MAX_GROUPS],
            slots_in_use: [const { AtomicBool::new(false) }; MAX_GROUPS],
            children: Mutex::new(HashMap::new()),
        }
    }

    /// Remember a spawned child so `wait_for` can reap it by pid.
    pub(crate) fn store_child(&self, pid: i32, child: std::process::Child) {
        self.children.lock_unpoisoned().insert(pid, child);
    }

    /// Take a spawned child to wait on it, if the pid names one.
    pub(crate) fn take_child(&self, pid: i32) -> Option<std::process::Child> {
        self.children.lock_unpoisoned().remove(&pid)
    }
}
