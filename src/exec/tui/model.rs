/// Runtime execution status of a single task.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TaskStatus {
    Pending,
    Running,
    Success,
    Error,
}

/// A single task within a chain: tracks its name, current status, and
/// accumulated output lines.
#[derive(Debug, Clone)]
pub(crate) struct TaskRow {
    pub(crate) name: String,
    pub(crate) status: TaskStatus,
    pub(crate) output: Vec<String>,
}

/// A chain of sequential tasks. Tasks are stored contiguously in `Model::tasks`
/// so `task_start`/`task_count` index into that flat vector.
#[derive(Debug, Clone)]
pub(crate) struct Chain {
    pub(crate) label: String,
    pub(crate) task_start: usize,
    pub(crate) task_count: usize,
}

/// All state tracked during a TUI session: a flat list of tasks with an
/// index structure (chains) that groups them into sequential groups.
#[derive(Debug, Clone)]
pub(crate) struct Model {
    pub(crate) tasks: Vec<TaskRow>,
    pub(crate) chains: Vec<Chain>,
}

impl Model {
    /// Create an empty model with no tasks or chains.
    pub(crate) fn new() -> Self {
        Self {
            tasks: Vec::new(),
            chains: Vec::new(),
        }
    }

    /// Register a new chain of sequential tasks by their display names.
    /// Each task starts in `Pending` status.
    pub(crate) fn add_chain(&mut self, label: String, task_names: Vec<String>) {
        let task_start = self.tasks.len();
        let task_count = task_names.len();
        for name in task_names {
            self.tasks.push(TaskRow {
                name,
                status: TaskStatus::Pending,
                output: Vec::new(),
            });
        }
        self.chains.push(Chain {
            label,
            task_start,
            task_count,
        });
    }

    /// Update the status of the task at `index`.
    pub(crate) fn update_task_status(&mut self, index: usize, status: TaskStatus) {
        if let Some(task) = self.tasks.get_mut(index) {
            task.status = status;
        }
    }

    /// Append a line of output text to the task at `idx`.
    pub(crate) fn append_output(&mut self, idx: usize, line: String) {
        if idx < self.tasks.len() {
            self.tasks[idx].output.push(line);
        }
    }

    /// True when every task has reached a terminal status (Success or Error).
    pub(crate) fn all_done(&self) -> bool {
        self.tasks
            .iter()
            .all(|t| matches!(t.status, TaskStatus::Success | TaskStatus::Error))
    }

    /// Count finalized tasks by outcome. Centralizes the success/error tally
    /// shared by the run and sync renderers so a new terminal status would
    /// propagate to both summaries from one place.
    pub(crate) fn success_and_error_counts(&self) -> (usize, usize) {
        let mut success_count = 0;
        let mut error_count = 0;
        for task in &self.tasks {
            match task.status {
                TaskStatus::Success => success_count += 1,
                TaskStatus::Error => error_count += 1,
                _ => {}
            }
        }
        (success_count, error_count)
    }

    /// Aggregate status for an entire chain: Error if any task failed,
    /// Running if any task is still active, Pending if nothing has started,
    /// Success otherwise.
    pub(crate) fn chain_status(&self, chain: &Chain) -> TaskStatus {
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
