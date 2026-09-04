/// Runtime execution status of a single task.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TaskStatus {
    Pending,
    Running,
    Success,
    Error,
    /// The task did not fail on its own: the run was already lost to
    /// another chain's failure, so its processes were killed (or it never
    /// started). Distinct from `Error` so the root cause stays visible.
    Cancelled,
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

    /// True when every task has reached a terminal status (Success, Error,
    /// or Cancelled).
    pub(crate) fn all_done(&self) -> bool {
        self.tasks.iter().all(|t| {
            matches!(
                t.status,
                TaskStatus::Success | TaskStatus::Error | TaskStatus::Cancelled
            )
        })
    }

    /// Aggregate status for an entire chain, derived from the same statuses
    /// its rows display so the header can never contradict them: Error if
    /// any task genuinely failed, Cancelled if the chain was stopped
    /// without a real failure, Running/Pending while work is outstanding,
    /// Success otherwise.
    pub(crate) fn chain_status(&self, chain: &Chain) -> TaskStatus {
        let mut has_error = false;
        let mut has_cancelled = false;
        let mut has_running = false;
        let mut has_pending = false;
        for i in chain.task_start..chain.task_start + chain.task_count {
            if let Some(task) = self.tasks.get(i) {
                match task.status {
                    TaskStatus::Error => has_error = true,
                    TaskStatus::Cancelled => has_cancelled = true,
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
        } else if has_cancelled {
            TaskStatus::Cancelled
        } else {
            TaskStatus::Success
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cancelled_chain_shows_cancelled_not_failed() {
        let mut model = Model::new();
        model.add_chain("a".to_string(), vec!["a1".to_string(), "a2".to_string()]);
        // Both tasks were victims of the fail-fast kill.
        model.update_task_status(0, TaskStatus::Cancelled);
        model.update_task_status(1, TaskStatus::Cancelled);

        // A cancelled task is terminal: the run can end.
        assert!(model.all_done());
        // The chain was stopped, not failed: its header must not claim a
        // failure its own rows never show.
        assert_eq!(model.chain_status(&model.chains[0]), TaskStatus::Cancelled);
    }

    #[test]
    fn a_real_failure_dominates_cancelled_members() {
        let mut model = Model::new();
        model.add_chain("a".to_string(), vec!["a1".to_string(), "a2".to_string()]);
        model.update_task_status(0, TaskStatus::Error);
        model.update_task_status(1, TaskStatus::Cancelled);

        assert!(model.all_done());
        assert_eq!(model.chain_status(&model.chains[0]), TaskStatus::Error);
    }
}
