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
/// depth, current status, and accumulated output lines.
#[derive(Debug, Clone)]
pub(crate) struct TaskRow {
    pub(crate) name: String,
    pub(crate) status: TaskStatus,
    pub(crate) output: Vec<String>,
    /// Nesting depth in the run's display plan.
    pub(crate) depth: usize,
    /// The `[project]` annotation this line executes under, when any.
    pub(crate) project: Option<String>,
}

/// A line selected for rendering, with tree prefixes computed over the
/// current visible set.
pub(crate) struct RenderRow<'a> {
    pub(crate) task: &'a TaskRow,
    pub(crate) prefix: String,
    pub(crate) output_prefix: String,
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
                    depth: line.depth,
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

    /// Replace the label of the task at `index` with its resolved text.
    pub(crate) fn update_task_label(&mut self, index: usize, name: String) {
        if let Some(task) = self.tasks.get_mut(index) {
            task.name = name;
        }
    }

    /// Set the resolved project annotation of the task at `index`.
    pub(crate) fn update_task_project(&mut self, index: usize, project: String) {
        if let Some(task) = self.tasks.get_mut(index) {
            task.project = Some(project);
        }
    }

    /// Append a line of output text to the task at `idx`.
    pub(crate) fn append_output(&mut self, idx: usize, line: String) {
        if idx < self.tasks.len() {
            self.tasks[idx].output.push(line);
        }
    }

    /// The lines the run display shows: a skipped switch arm and its body
    /// are not part of what ran, so they are dropped. Tree prefixes are
    /// recomputed over exactly these lines so no branch points at a hidden
    /// one.
    pub(crate) fn visible_rows(&self) -> Vec<RenderRow<'_>> {
        let visible: Vec<&TaskRow> = self
            .tasks
            .iter()
            .filter(|task| task.status != TaskStatus::Skipped)
            .collect();
        let depths: Vec<usize> = visible.iter().map(|task| task.depth).collect();
        visible
            .into_iter()
            .zip(crate::ir::tree_prefixes(&depths))
            .map(|(task, (prefix, output_prefix))| RenderRow {
                task,
                prefix,
                output_prefix,
            })
            .collect()
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

    fn plan_line(row: usize, depth: usize, label: &str) -> PlanLine {
        PlanLine {
            row,
            depth,
            label: label.to_string(),
            project: None,
            prefix: String::new(),
            output_prefix: String::new(),
        }
    }

    #[test]
    fn model_keeps_row_position_and_project() {
        let mut plan = vec![plan_line(0, 0, "async"), plan_line(1, 1, "exec cargo test")];
        plan[1].project = Some("kiru".to_string());
        let model = Model::from_plan(plan);
        assert_eq!(model.tasks[1].depth, 1);
        assert_eq!(model.tasks[1].project.as_deref(), Some("kiru"));
        assert_eq!(model.tasks.len(), 2, "line position equals row index");
    }

    #[test]
    fn resolved_label_and_project_replace_plan_text() {
        let mut model = Model::from_plan(vec![plan_line(0, 0, "exec: echo $(date)")]);
        model.update_task_label(0, "exec: echo Fri".to_string());
        model.update_task_project(1, "kiru".to_string());
        assert_eq!(model.tasks[0].name, "exec: echo Fri");
        assert_eq!(
            model.tasks[0].project, None,
            "an out-of-range row update is ignored"
        );
        model.update_task_project(0, "app".to_string());
        assert_eq!(model.tasks[0].project.as_deref(), Some("app"));
    }

    #[test]
    fn visible_rows_drop_skipped_arm_and_repair_branches() {
        let mut model = Model::from_plan(vec![
            plan_line(0, 0, "switch case a"),
            plan_line(1, 1, "exec taken"),
            plan_line(2, 0, "switch case b"),
            plan_line(3, 1, "exec untaken"),
            plan_line(4, 0, "log: after"),
        ]);
        model.update_task_status(0, TaskStatus::Success);
        model.update_task_status(1, TaskStatus::Success);
        model.update_task_status(2, TaskStatus::Skipped);
        model.update_task_status(3, TaskStatus::Skipped);
        model.update_task_status(4, TaskStatus::Success);

        let visible = model.visible_rows();
        let names: Vec<&str> = visible.iter().map(|row| row.task.name.as_str()).collect();
        assert_eq!(names, vec!["switch case a", "exec taken", "log: after"]);
        assert_eq!(
            visible[1].prefix, "│  └─ ",
            "the taken arm is the only child but the arm line is not last"
        );
    }
}
