pub(crate) mod chain;
pub(crate) mod colors;
pub(crate) mod context;
pub(crate) mod error;
pub(crate) mod executor;
pub(crate) mod subprocess;
pub(crate) mod sync;
pub(crate) mod tui;

#[cfg(test)]
mod test_support;

pub(crate) use context::OutputCallback;
pub(crate) use executor::Executor;
pub(crate) use tui::{TaskStatus, TuiEvent, run_tui_with_run, run_tui_with_sync, send_tui_event};

use crate::exec::error::RuntimeError;
use tokio::sync::mpsc;
use tokio::task::{JoinError, JoinHandle};

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
/// Centralizes the success/error/panic status reporting shared by the chain
/// and sync runners. The error is borrowed so callers keep ownership and can
/// propagate it further.
pub(crate) fn report_task_outcome(
    tx: &mpsc::UnboundedSender<TuiEvent>,
    index: usize,
    outcome: TaskOutcome<&error::RuntimeError>,
) -> bool {
    match outcome {
        TaskOutcome::Success => {
            send_tui_event(tx, TuiEvent::UpdateStatus(index, TaskStatus::Success));
            false
        }
        TaskOutcome::Error(e) => {
            // Timeout errors are already emitted via OutputCallback inside
            // ExecContext::run_live with correct shell indent, suppress
            // the duplicate "Error:" line here.
            let is_timeout = e.is_timeout();
            if !is_timeout {
                send_tui_event(tx, TuiEvent::AppendOutput(index, format!("Error: {}", e)));
            }
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

/// Await all spawned task handles and reduce their results to a single
/// aggregate error message.
///
/// Success and error outcomes are reported by each task's own blocking
/// closure (chain tasks report per-step as they progress, sync tasks report
/// when a project finishes), so this driver only joins the handles, reports
/// panics the closures had no chance to surface, and fails with
/// `failure_message` when any task failed or panicked.
pub(crate) async fn await_tasks_and_report(
    tx: &mpsc::UnboundedSender<TuiEvent>,
    task_handles: Vec<(usize, JoinHandle<Result<(), RuntimeError>>)>,
    failure_message: &str,
) -> Result<(), String> {
    let mut any_failed = false;
    for (task_index, handle) in task_handles {
        match handle.await {
            Ok(Ok(())) => {}
            Ok(Err(_)) => any_failed = true,
            Err(join_error) => {
                if report_task_outcome(
                    tx,
                    task_index,
                    TaskOutcome::<&RuntimeError>::Panic(join_error),
                ) {
                    any_failed = true;
                }
            }
        }
    }
    if any_failed {
        Err(failure_message.to_string())
    } else {
        Ok(())
    }
}
