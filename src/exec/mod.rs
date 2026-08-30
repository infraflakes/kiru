pub(crate) mod chain;
pub(crate) mod colors;
pub(crate) mod context;
pub(crate) mod error;
pub(crate) mod executor;
pub(crate) mod sync;
pub(crate) mod tui;

#[cfg(test)]
mod test_support;

pub(crate) use context::OutputCallback;
pub(crate) use executor::Executor;
pub(crate) use tui::{TaskStatus, TuiEvent, run_tui_with_run, run_tui_with_sync, send_tui_event};

/// Whether the current invocation was started with `KIRU_CWD=1`.
///
/// When this is set, project-body `exec` commands run in the process's
/// current working directory (the assumption is the caller already `cd`'d
/// into the project, e.g. in CI). When unset, they run in the project's own
/// directory[^1] instead.
///
/// [^1]: `project.dir`, resolved from the project's `dir` field.
pub(crate) fn kiru_cwd_enabled() -> bool {
    std::env::var("KIRU_CWD").as_deref() == Ok("1")
}

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
/// This removes the duplicated success/error/panic reporting that previously
/// lived inline in both the chain and sync runners. The emitted output text is
/// preserved exactly so existing TUI behavior stays unchanged. The error is
/// borrowed so callers keep ownership of the underlying value and can
/// propagate it further.
pub(crate) fn report_task_outcome<E: std::fmt::Display>(
    tx: &mpsc::UnboundedSender<TuiEvent>,
    index: usize,
    outcome: TaskOutcome<&E>,
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
) -> miette::Result<()> {
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
        Err(miette::miette!("{}", failure_message))
    } else {
        Ok(())
    }
}
