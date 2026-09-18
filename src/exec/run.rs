//! Run-block execution: drives one run body through the TUI.
//!
//! A run body is an instruction tree whose direct statements are display
//! rows (`Instruction::Task`) and whose `async` statements are concurrent
//! groups. The executor joins every async body at the end of the body that
//! started it, so a run is structured: nothing keeps running behind a
//! finished or failed body.

use super::context::{
    ExecContext, LabelCallback, OutputCallback, ProjectCallback, ProjectExec, RowReporter,
    RunContext, StatusCallback,
};
use super::subprocess::RunKillSwitch;
use super::{TaskRunError, format_final_output, render_run_output, send_tui_event};
use crate::exec::error::RuntimeError;
use crate::ir::Instruction;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;

/// Execute a compiled run body, reporting rows through the TUI.
///
/// `plan` is the run's display plan from [`crate::ir::Ir::run_plan`]: every
/// step and switch arm with its indentation and project annotation. Model
/// line position equals the compile-assigned row index, so status and
/// output events address lines directly.
pub(crate) fn execute_run(
    body: Vec<Instruction>,
    plan: Vec<crate::ir::PlanLine>,
    projects: Arc<BTreeMap<String, ProjectExec>>,
    invocation_cwd: PathBuf,
    shell: String,
    timeout: Option<Duration>,
) -> Result<(), TaskRunError> {
    let kill = Arc::new(RunKillSwitch::new());
    let kill_for_cancel = Arc::clone(&kill);
    let run = Arc::new(RunContext {
        shell,
        timeout,
        kill: Some(Arc::clone(&kill)),
        projects,
    });

    let worker = move |tx: mpsc::UnboundedSender<super::TuiEvent>| async move {
        let output_tx = tx.clone();
        let status_tx = tx.clone();
        let label_tx = tx.clone();
        let project_tx = tx;
        let worker = tokio::task::spawn_blocking(move || {
            let output: OutputCallback = Arc::new(move |row, line| {
                if let Some(row) = row {
                    send_tui_event(&output_tx, super::TuiEvent::AppendOutput(row, line));
                }
            });
            let status: StatusCallback = Arc::new(move |row, status| {
                send_tui_event(&status_tx, super::TuiEvent::UpdateStatus(row, status));
            });
            let label: LabelCallback = Arc::new(move |row, name| {
                send_tui_event(&label_tx, super::TuiEvent::UpdateLabel(row, name));
            });
            let project: ProjectCallback = Arc::new(move |row, name| {
                send_tui_event(&project_tx, super::TuiEvent::UpdateProject(row, name));
            });
            let reporter = RowReporter {
                output,
                status,
                label,
                project,
            };
            let root = ExecContext::new(run, reporter, invocation_cwd, false);
            let mut root = root;
            root.exec_stmts(&body)
        });
        worker
            .await
            .map_err(|e| RuntimeError::exec_io_error("run", e))?
    };

    match super::run_tui_with(
        plan,
        worker,
        render_run_output,
        Some(format_final_output),
        Some(kill_for_cancel),
    ) {
        // Every failing row already rendered its own error; only the exit
        // code remains.
        Ok(Ok(())) => Ok(()),
        Ok(Err(_error)) => Err(TaskRunError::TaskFailed),
        Err(message) => Err(TaskRunError::Infrastructure(message)),
    }
}
