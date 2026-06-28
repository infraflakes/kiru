use std::sync::{Arc, Mutex};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskStatus {
    Pending,
    Running,
    Success,
    Error,
}

#[derive(Debug, Clone)]
pub(crate) struct Task {
    pub name: String,
    pub status: TaskStatus,
    pub output: Vec<String>,
    pub finalized: bool,
}

#[derive(Debug, Clone)]
pub(crate) struct Chain {
    pub label: String,
    pub task_start: usize,
    pub task_count: usize,
}

#[derive(Debug, Clone)]
pub struct Model {
    pub tasks: Vec<Task>,
    pub chains: Vec<Chain>,
}

impl Model {
    pub fn new() -> Self {
        Self {
            tasks: Vec::new(),
            chains: Vec::new(),
        }
    }

    pub(super) fn lock(arc: &Arc<Mutex<Model>>) -> std::sync::MutexGuard<'_, Model> {
        arc.lock().unwrap_or_else(|e| e.into_inner())
    }

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

    pub fn update_task_status(&mut self, index: usize, status: TaskStatus) {
        if let Some(task) = self.tasks.get_mut(index) {
            task.status = status;
            task.finalized = matches!(status, TaskStatus::Success | TaskStatus::Error);
        }
    }

    pub fn append_output(&mut self, idx: usize, line: String) {
        if idx < self.tasks.len() {
            self.tasks[idx].output.push(line);
        }
    }

    pub fn all_done(&self) -> bool {
        self.tasks
            .iter()
            .all(|t| matches!(t.status, TaskStatus::Success | TaskStatus::Error))
    }

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
