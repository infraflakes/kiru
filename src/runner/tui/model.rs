use std::sync::{Arc, Mutex};

/// Runtime execution status of a single task.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskStatus {
    Pending,
    Running,
    Success,
    Error,
}

/// A single task within a chain: tracks its name, current status, accumulated
/// output lines, and whether it has finished.
#[derive(Debug, Clone)]
pub(crate) struct Task {
    pub name: String,
    pub status: TaskStatus,
    pub output: Vec<String>,
    pub finalized: bool,
}

/// A chain of sequential tasks. Tasks are stored contiguously in `Model::tasks`
/// so `task_start`/`task_count` index into that flat vector.
#[derive(Debug, Clone)]
pub(crate) struct Chain {
    pub label: String,
    pub task_start: usize,
    pub task_count: usize,
}

/// All state tracked during a TUI session: a flat list of tasks with an
/// index structure (chains) that groups them into sequential groups.
#[derive(Debug, Clone)]
pub struct Model {
    pub tasks: Vec<Task>,
    pub chains: Vec<Chain>,
}

impl Model {
    /// Create an empty model with no tasks or chains.
    pub fn new() -> Self {
        Self {
            tasks: Vec::new(),
            chains: Vec::new(),
        }
    }

    /// Lock the model behind an `Arc<Mutex<>>` and return a guard.
    /// Recovers from poisoned mutexes by taking ownership of the data.
    pub(super) fn lock(arc: &Arc<Mutex<Model>>) -> std::sync::MutexGuard<'_, Model> {
        arc.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Register a new chain of sequential tasks by their display names.
    /// Each task starts in `Pending` status.
    pub fn add_chain(&mut self, label: String, task_names: Vec<String>) {
        let task_start = self.tasks.len();
        let task_count = task_names.len();
        for name in task_names {
            self.tasks.push(Task {
                name,
                status: TaskStatus::Pending,
                output: Vec::new(),
                finalized: false,
            });
        }
        self.chains.push(Chain {
            label,
            task_start,
            task_count,
        });
    }

    /// Update the status of the task at `index`. Terminal states
    /// (Success, Error) mark the task as finalized.
    pub fn update_task_status(&mut self, index: usize, status: TaskStatus) {
        if let Some(task) = self.tasks.get_mut(index) {
            task.status = status;
            task.finalized = matches!(status, TaskStatus::Success | TaskStatus::Error);
        }
    }

    /// Append a line of output text to the task at `idx`.
    pub fn append_output(&mut self, idx: usize, line: String) {
        if idx < self.tasks.len() {
            self.tasks[idx].output.push(line);
        }
    }

    /// True when every task has reached a terminal status (Success or Error).
    pub fn all_done(&self) -> bool {
        self.tasks
            .iter()
            .all(|t| matches!(t.status, TaskStatus::Success | TaskStatus::Error))
    }

    /// Aggregate status for an entire chain: Error if any task failed,
    /// Running if any task is still active, Pending if nothing has started,
    /// Success otherwise.
    pub fn chain_status(&self, chain: &Chain) -> TaskStatus {
        let mut has_error = false;
        let mut has_running = false;
        let mut has_pending = false;
        for i in chain.task_start..chain.task_start + chain.task_count {
            if let Some(task) = self.tasks.get(i) {
                match task.status {
                    TaskStatus::Error => has_error = true,
                    TaskStatus::Running => has_running = true,
                    TaskStatus::Pending => has_pending = true,
                    _ => {}
                }
            }
        }
        if has_error {
            TaskStatus::Error
        } else if has_running {
            TaskStatus::Running
        } else if has_pending {
            TaskStatus::Pending
        } else {
            TaskStatus::Success
        }
    }
}
