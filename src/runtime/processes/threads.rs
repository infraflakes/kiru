//! The thread table: the threads `async` has started.
//!
//! Each spawned thread is remembered together with the id of the thread that
//! spawned it, so `wait` joins exactly the caller's own children.

use std::sync::Mutex;
use std::thread::{JoinHandle, ThreadId};

use crate::cli::lock_recovery::MutexExt;

/// The threads `async` has started, keyed by their spawner.
pub(crate) struct ThreadTable {
    threads: Mutex<Vec<ChildThread>>,
}

/// One thread `async` started, remembered together with the id of the
/// thread that spawned it. The pair lets `wait` join exactly the
/// caller's own children.
struct ChildThread {
    parent: ThreadId,
    join_handle: JoinHandle<()>,
}

impl ThreadTable {
    pub(crate) fn new() -> Self {
        Self {
            threads: Mutex::new(Vec::new()),
        }
    }

    /// Start `body` on a new OS thread and record the spawning thread as its
    /// parent. Nothing joins it until a wait on the parent or the end of the
    /// run.
    pub(crate) fn spawn(&self, body: impl FnOnce() + Send + 'static) {
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
    pub(crate) fn join_all(&self) {
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
}
