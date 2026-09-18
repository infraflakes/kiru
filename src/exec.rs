pub(crate) mod colors;
pub(crate) mod context;
pub(crate) mod direnv;
pub(crate) mod error;
pub(crate) mod model;
pub(crate) mod run;
pub(crate) mod signals;
pub(crate) mod subprocess;
pub(crate) mod sync;
pub(crate) mod tui;

pub(crate) use context::ProjectExec;
pub(crate) use run::execute_run;
pub(crate) use sync::{ProjectSync, run_sync_for_projects};
pub(crate) use tui::run::{format_final_output, render_run_output};
pub(crate) use tui::sync::render_sync_output;

/// Why a TUI-driven batch of tasks (a run block or a sync) ended
/// unsuccessfully. The distinction matters for reporting: task failures
/// were already rendered through the display, infrastructure failures were
/// not.
pub(crate) enum TaskRunError {
    /// At least one task failed. Every failing task rendered its own error
    /// through the display, so there is nothing left to print, only a
    /// non-zero exit remains.
    TaskFailed,
    /// The TUI or worker infrastructure failed before or while running the
    /// tasks. The message has not been shown anywhere yet.
    Infrastructure(String),
}
