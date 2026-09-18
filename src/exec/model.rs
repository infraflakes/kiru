//! The runtime display state of one run or sync: one state per node, written
//! directly by the executor and read by the renderers.
//!
//! There is no second model and no event stream. A [`NodeId`] names a node in
//! the program arena; the same id names its [`NodeState`] here, so execution
//! and display are two views of one structure.

use crate::exec::colors;
use crate::ir::Program;
use ratatui::style::Color;

/// Spinner frames shown on a running row, one per redraw.
pub(crate) const SPINNER_FRAMES: &[char] = &['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];

/// Runtime execution status of a single task.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub(crate) enum TaskStatus {
    #[default]
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

impl TaskStatus {
    /// The one-character marker of this status.
    pub(crate) fn glyph(self, spinner_idx: usize) -> char {
        match self {
            TaskStatus::Success => '✓',
            TaskStatus::Error => '✗',
            TaskStatus::Cancelled => '■',
            TaskStatus::Skipped => '-',
            TaskStatus::Pending => '·',
            TaskStatus::Running => SPINNER_FRAMES[spinner_idx % SPINNER_FRAMES.len()],
        }
    }

    /// The ratatui color of this status, for the live view.
    pub(crate) fn color(self) -> Color {
        match self {
            TaskStatus::Success => colors::OK,
            TaskStatus::Running => colors::RUNNING,
            TaskStatus::Pending => colors::PENDING,
            TaskStatus::Error => colors::FAILED,
            TaskStatus::Cancelled => colors::CANCELLED,
            TaskStatus::Skipped => colors::PENDING,
        }
    }

    /// The ANSI color of this status, for the final dump.
    pub(crate) fn ansi(self) -> &'static str {
        match self {
            TaskStatus::Success => colors::OK_ANSI,
            TaskStatus::Running => colors::BRIGHT_YELLOW_ANSI,
            TaskStatus::Pending => colors::GRAY_ANSI,
            TaskStatus::Error => colors::FAILED_ANSI,
            TaskStatus::Cancelled => colors::CANCELLED_ANSI,
            TaskStatus::Skipped => colors::GRAY_ANSI,
        }
    }
}

/// Runtime state of one node: status, captured output, and the values that
/// were only known once the node ran (resolved label, resolved project).
#[derive(Debug, Clone, Default)]
pub(crate) struct NodeState {
    pub(crate) status: TaskStatus,
    pub(crate) output: Vec<String>,
    /// Resolved label override; `None` derives the label from the node kind.
    pub(crate) label: Option<String>,
    /// Resolved project annotation; `None` means no project or a literal one.
    pub(crate) project: Option<String>,
}

/// The states of every node of one program, indexed by node id.
#[derive(Debug, Clone, Default)]
pub(crate) struct Display {
    states: Vec<NodeState>,
}

impl Display {
    /// One pending state per program node, so every id addresses its state.
    pub(crate) fn for_program(program: &Program) -> Self {
        Self {
            states: vec![NodeState::default(); program.nodes.len()],
        }
    }

    /// One state per flat row with a preset label (the sync view, which has
    /// no program nodes of its own).
    pub(crate) fn with_labels(labels: Vec<String>) -> Self {
        Self {
            states: labels
                .into_iter()
                .map(|label| NodeState {
                    label: Some(label),
                    ..NodeState::default()
                })
                .collect(),
        }
    }

    /// The number of rows, hidden ones included.
    pub(crate) fn len(&self) -> usize {
        self.states.len()
    }

    /// The state of one node.
    pub(crate) fn state(&self, id: usize) -> &NodeState {
        &self.states[id]
    }

    /// Every state in row order (the flat sync view walks these directly).
    pub(crate) fn states(&self) -> &[NodeState] {
        &self.states
    }

    pub(crate) fn set_status(&mut self, id: usize, status: TaskStatus) {
        self.states[id].status = status;
    }

    pub(crate) fn push_output(&mut self, id: usize, line: String) {
        self.states[id].output.push(line);
    }

    pub(crate) fn set_label(&mut self, id: usize, label: String) {
        self.states[id].label = Some(label);
    }

    pub(crate) fn set_project(&mut self, id: usize, project: String) {
        self.states[id].project = Some(project);
    }

    /// True when every row is terminal: success, error, cancelled, or
    /// skipped.
    pub(crate) fn all_done(&self) -> bool {
        self.states.iter().all(|state| {
            matches!(
                state.status,
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

    #[test]
    fn skipped_rows_are_terminal() {
        let mut display = Display::with_labels(vec!["a".to_string(), "b".to_string()]);
        display.set_status(0, TaskStatus::Success);
        display.set_status(1, TaskStatus::Skipped);
        assert!(
            display.all_done(),
            "a skipped switch arm is terminal, not pending"
        );
    }

    #[test]
    fn state_updates_are_addressed_by_id() {
        let mut display = Display::with_labels(vec!["row".to_string()]);
        display.set_status(0, TaskStatus::Running);
        display.push_output(0, "line".to_string());
        display.set_label(0, "resolved".to_string());
        display.set_project(0, "kiru".to_string());
        let state = display.state(0);
        assert_eq!(state.status, TaskStatus::Running);
        assert_eq!(state.output, vec!["line".to_string()]);
        assert_eq!(state.label.as_deref(), Some("resolved"));
        assert_eq!(state.project.as_deref(), Some("kiru"));
    }
}
