//! The run state shared by every body: lifecycle flags and the thread table.
//!
//! The signal flags and the process-group table live here too, because a
//! signal handler and the waiting loop both read them; `signals` owns the
//! handlers and the group-table operations.

use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use std::thread::{JoinHandle, ThreadId};

use crate::lock_recovery::MutexExt;

/// The most process groups one run can track at once. A run that exceeds it
/// fails rather than silently leaking an untracked group.
pub(super) const MAX_GROUPS: usize = 1024;

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
    pub(super) group_ids: [AtomicI32; MAX_GROUPS],
    pub(super) slots_in_use: [AtomicBool; MAX_GROUPS],
    threads: Mutex<Vec<ChildThread>>,
    /// Spawned processes waiting to be reaped by `wait_for`, keyed by pid.
    children: Mutex<HashMap<i32, std::process::Child>>,
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
            cancelled: AtomicBool::new(false),
            panicked: AtomicBool::new(false),
            group_ids: [const { AtomicI32::new(0) }; MAX_GROUPS],
            slots_in_use: [const { AtomicBool::new(false) }; MAX_GROUPS],
            threads: Mutex::new(Vec::new()),
            children: Mutex::new(HashMap::new()),
        }
    }

    /// Remember a spawned child so `wait_for` can reap it by pid.
    pub(super) fn store_child(&self, pid: i32, child: std::process::Child) {
        self.children.lock_unpoisoned().insert(pid, child);
    }

    /// Take a spawned child to wait on it, if the pid names one.
    pub(super) fn take_child(&self, pid: i32) -> Option<std::process::Child> {
        self.children.lock_unpoisoned().remove(&pid)
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
