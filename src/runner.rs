pub(crate) mod chain;
pub(crate) mod colors;
pub(crate) mod error;
pub(crate) mod execution_context;
pub(crate) mod runner_impl;
pub(crate) mod sync;
pub(crate) mod tui;

#[cfg(test)]
mod tests;

pub(crate) use execution_context::OutputCallback;
pub(crate) use runner_impl::Runner;
pub(crate) use tui::{TaskStatus, TuiEvent, run_tui_with_run, run_tui_with_sync, send_tui_event};
