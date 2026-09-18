/// Runtime execution status of a single task.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum TaskStatus {
    Pending,
    Running,
    Success,
    Error,
    /// The task did not fail on its own: the run was already lost to
    /// another task's failure, so its processes were killed (or it never
    /// started). Distinct from `Error` so the root cause stays visible.
    Cancelled,
    /// The task never ran by design: a switch arm that was not taken.
    Skipped,
}

/// One rendered line of a run: a step or a switch arm. Tracks its display
/// metadata, current status, and accumulated output lines.
#[derive(Debug, Clone)]
pub(crate) struct TaskRow {
    pub(crate) name: String,
    pub(crate) status: TaskStatus,
    pub(crate) output: Vec<String>,
    /// Tree-branch prefix from the display plan.
    pub(crate) prefix: String,
    /// Prefix for this line's output block, in the final dump.
    pub(crate) output_prefix: String,
    /// The `[project]` annotation this line executes under, when any.
    pub(crate) project: Option<String>,
}

/// All state tracked during a TUI session: a flat list of display lines in
/// row order. Line position equals the compile-assigned row index, so
/// status and output events address lines directly.
#[derive(Debug, Clone)]
pub(crate) struct Model {
    pub(crate) tasks: Vec<TaskRow>,
}

impl Model {
    /// Build the model from a run's display plan. Every line starts
    /// `Pending`, in compile-assigned row order.
    pub(crate) fn from_plan(plan: Vec<crate::ir::PlanLine>) -> Self {
        Self {
            tasks: plan
                .into_iter()
                .map(|line| TaskRow {
                    name: line.label,
                    status: TaskStatus::Pending,
                    output: Vec::new(),
                    prefix: line.prefix,
                    output_prefix: line.output_prefix,
                    project: line.project,
                })
                .collect(),
        }
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

    /// True when every line has reached a terminal status (Success, Error,
    /// Cancelled, or Skipped).
    pub(crate) fn all_done(&self) -> bool {
        self.tasks.iter().all(|t| {
            matches!(
                t.status,
                TaskStatus::Success
                    | TaskStatus::Error
                    | TaskStatus::Cancelled
                    | TaskStatus::Skipped
            )
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ir::PlanLine;

    #[test]
    fn skipped_lines_are_terminal() {
        let mut model = Model::from_plan(vec![
            PlanLine {
                row: 0,
                depth: 0,
                label: "switch case a".to_string(),
                project: None,
                prefix: "├─ ".to_string(),
                output_prefix: "│    ".to_string(),
            },
            PlanLine {
                row: 1,
                depth: 0,
                label: "switch default".to_string(),
                project: None,
                prefix: "└─ ".to_string(),
                output_prefix: "     ".to_string(),
            },
        ]);
        model.update_task_status(0, TaskStatus::Success);
        model.update_task_status(1, TaskStatus::Skipped);
        assert!(
            model.all_done(),
            "a skipped switch arm is terminal, not pending"
        );
    }

    #[test]
    fn model_lines_carry_prefix_and_project() {
        let plan = vec![
            PlanLine {
                row: 0,
                depth: 0,
                label: "async".to_string(),
                project: Some("kiru".to_string()),
                prefix: "└─ ".to_string(),
                output_prefix: "     ".to_string(),
            },
            PlanLine {
                row: 1,
                depth: 1,
                label: "exec cargo test".to_string(),
                project: Some("kiru".to_string()),
                prefix: "   └─ ".to_string(),
                output_prefix: "        ".to_string(),
            },
        ];
        let model = Model::from_plan(plan);
        assert_eq!(model.tasks[1].prefix, "   └─ ");
        assert_eq!(model.tasks[1].project.as_deref(), Some("kiru"));
        assert_eq!(model.tasks.len(), 2, "line position equals row index");
    }
}
