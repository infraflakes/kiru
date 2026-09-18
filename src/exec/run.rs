//! Run-block execution: drives one run body through the TUI.
//!
//! The program is compiled into an arena of statement nodes; the executor
//! walks one run's nodes and writes their runtime state into the shared
//! display, which the renderers read back. Async nodes are joined at the end
//! of the body that started them, so a run is structured: nothing keeps
//! running behind a finished or failed body.

use super::context::{ExecContext, ProjectExec, RunContext};
use super::model::Display;
use super::subprocess::RunKillSwitch;
use super::{TaskRunError, format_final_output, render_run_output};
use crate::ir::Program;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Execute one compiled run body, reporting every node through the shared
/// display.
pub(crate) fn execute_run(
    program: Arc<Program>,
    run: &str,
    projects: Arc<BTreeMap<String, ProjectExec>>,
    invocation_cwd: PathBuf,
    shell: String,
    timeout: Option<Duration>,
) -> Result<(), TaskRunError> {
    let body = program.run_children(run).to_vec();
    let kill = Arc::new(RunKillSwitch::new());
    let kill_for_cancel = Arc::clone(&kill);
    let display = Arc::new(Mutex::new(Display::for_program(&program)));

    let program_for_worker = Arc::clone(&program);
    let display_for_worker = Arc::clone(&display);
    let run_context = Arc::new(RunContext {
        shell,
        timeout,
        kill: Some(Arc::clone(&kill)),
        projects,
    });
    let worker = move || {
        let mut context = ExecContext::new(
            &program_for_worker,
            display_for_worker,
            run_context,
            invocation_cwd,
            false,
        );
        context.exec_children(&body)
    };

    let program_for_view = Arc::clone(&program);
    let render = move |frame: &mut ratatui::Frame, display: &Display, spinner_idx: usize| {
        render_run_output(frame, &program_for_view, display, run, spinner_idx);
    };
    let program_for_dump = Arc::clone(&program);
    let format = move |display: &Display| format_final_output(&program_for_dump, display, run);

    match super::tui::run_tui(
        display,
        program.run_row_count(run),
        worker,
        render,
        Some(format),
        Some(kill_for_cancel),
    ) {
        // Every failing row already rendered its own error; only the exit
        // code remains.
        Ok(Ok(())) => Ok(()),
        Ok(Err(_error)) => Err(TaskRunError::TaskFailed),
        Err(message) => Err(TaskRunError::Infrastructure(message)),
    }
}
