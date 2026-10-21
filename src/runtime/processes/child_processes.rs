//! The process table: the live process groups and the spawned children.
//!
//! The operating system allocates process resources, so the table grows with
//! what a run spawns and has no fixed cap. A signal is delivered through a
//! `signalfd` to a monitor thread, not to an asynchronous handler, so the group
//! list is an ordinary mutex-guarded Vec: a signal path may allocate and lock,
//! and every read of the table happens on a normal thread.

use std::collections::HashMap;
use std::sync::Mutex;

use crate::sync::MutexExt;

/// The live process groups and the spawned processes waiting to be reaped by
/// `wait_for`.
pub(crate) struct ProcessTable {
    /// The process group each live spawn leads, in spawn order.
    groups: Mutex<Vec<i32>>,
    /// Spawned processes waiting to be reaped by `wait_for`, keyed by pid.
    children: Mutex<HashMap<i32, std::process::Child>>,
}

impl ProcessTable {
    pub(crate) fn new() -> Self {
        Self {
            groups: Mutex::new(Vec::new()),
            children: Mutex::new(HashMap::new()),
        }
    }

    /// Remember a spawned process group.
    pub(super) fn register(&self, pgid: i32) {
        self.groups.lock_unpoisoned().push(pgid);
    }

    /// Forget a process group whose leader has been reaped.
    pub(super) fn unregister(&self, pgid: i32) {
        let mut groups = self.groups.lock_unpoisoned();
        if let Some(index) = groups.iter().position(|group| *group == pgid) {
            groups.swap_remove(index);
        }
    }

    /// A snapshot of the live process groups, so a stop may signal them without
    /// holding the table's lock across a syscall.
    pub(super) fn live_groups(&self) -> Vec<i32> {
        self.groups.lock_unpoisoned().clone()
    }

    /// The number of live process groups. Serves the tests that prove the table
    /// has no fixed cap.
    #[cfg(test)]
    pub(super) fn group_count(&self) -> usize {
        self.groups.lock_unpoisoned().len()
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
