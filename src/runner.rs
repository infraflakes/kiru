pub(crate) mod chain;
pub(crate) mod colors;
pub(crate) mod error;
pub(crate) mod execution_context;
pub(crate) mod runner_impl;
pub(crate) mod sync;
pub(crate) mod tui;

#[cfg(test)]
mod test_support;

pub(crate) use execution_context::OutputCallback;
pub(crate) use runner_impl::Runner;
pub(crate) use tui::{TaskStatus, TuiEvent, run_tui_with_run, run_tui_with_sync, send_tui_event};

use tokio::sync::mpsc;
use tokio::task::JoinError;

/// Outcome of a single async task (a chain step or a project sync), used to
/// centralize the TUI event reporting shared by the chain and sync runners.
pub(crate) enum TaskOutcome<E> {
    /// Completed successfully.
    Success,
    /// Returned an error before finishing.
    Error(E),
    /// The spawned task panicked and could not be joined normally.
    Panic(JoinError),
}

/// Emits the TUI events for a finished task and returns whether it failed.
///
/// This removes the duplicated success/error/panic reporting that previously
/// lived inline in both the chain and sync runners. The emitted output text is
/// preserved exactly so existing TUI behavior stays unchanged.
pub(crate) fn report_task_outcome<E: std::fmt::Display>(
    tx: &mpsc::UnboundedSender<TuiEvent>,
    index: usize,
    outcome: TaskOutcome<E>,
) -> bool {
    match outcome {
        TaskOutcome::Success => {
            send_tui_event(tx, TuiEvent::UpdateStatus(index, TaskStatus::Success));
            false
        }
        TaskOutcome::Error(e) => {
            send_tui_event(tx, TuiEvent::AppendOutput(index, format!("Error: {}", e)));
            send_tui_event(tx, TuiEvent::UpdateStatus(index, TaskStatus::Error));
            true
        }
        TaskOutcome::Panic(e) => {
            send_tui_event(
                tx,
                TuiEvent::AppendOutput(index, format!("Task panicked: {}", e)),
            );
            send_tui_event(tx, TuiEvent::UpdateStatus(index, TaskStatus::Error));
            true
        }
    }
}
