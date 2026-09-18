//! Terminal shell shared by `run` and `sync`.
//!
//! With a terminal on stdout the live inline view draws as the worker
//! progresses; without one (pipes, CI) the run is headless and only the
//! final dump is written, so logs stay free of cursor-control frames. Both
//! paths watch the shutdown signal and stop every live child group on
//! cancel.

use crate::exec::model::{Display, SPINNER_FRAMES};
use crate::exec::signals;
use crossterm::{
    event::{self, Event, KeyCode, KeyModifiers},
    terminal::{disable_raw_mode, enable_raw_mode},
};
use ratatui::{Frame, Terminal, TerminalOptions, Viewport, backend::CrosstermBackend};
use std::io::{self, IsTerminal, Write};
use std::sync::{Arc, Mutex};
use std::time::Duration;

pub(super) mod run;
pub(super) mod sync;

/// Rows of captured output shown per row in the final dump.
pub(super) const MAX_PANEL_HEIGHT: usize = 15;

/// How long one frame stays up before a redraw, and the keyboard poll window.
const REDRAW_INTERVAL: Duration = Duration::from_millis(50);

/// RAII toggle for terminal raw mode, active only while keys are read.
struct RawMode;

impl RawMode {
    fn enable() -> Option<Self> {
        enable_raw_mode().ok().map(|()| RawMode)
    }
}

impl Drop for RawMode {
    fn drop(&mut self) {
        let _ = disable_raw_mode();
    }
}

/// Scroll past the inline viewport and write the final formatted output.
fn dump_final_output(height: u16, dump: &str) -> Result<(), io::Error> {
    let mut out = io::stdout().lock();
    for _ in 0..height {
        out.write_all(b"\n")?;
    }
    out.flush()?;
    out.write_all(dump.as_bytes())?;
    out.flush()?;
    Ok(())
}

/// Write the final dump directly, for a headless run with no viewport to
/// scroll past.
fn write_dump(dump: &str) -> Result<(), io::Error> {
    let mut out = io::stdout().lock();
    out.write_all(dump.as_bytes())?;
    out.flush()
}

/// Poll for keyboard input for one frame interval. Returns `true` when the
/// user requested exit (`q` or Ctrl+C).
fn cancel_key_pressed() -> bool {
    if matches!(event::poll(REDRAW_INTERVAL), Ok(true))
        && let Ok(Event::Key(key)) = event::read()
    {
        matches!(key.code, KeyCode::Char('q'))
            || (key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL))
    } else {
        false
    }
}

/// Draw one frame from the shared display.
fn draw(
    terminal: &mut Terminal<CrosstermBackend<io::Stdout>>,
    display: &Arc<Mutex<Display>>,
    render: &impl Fn(&mut Frame, &Display, usize),
    spinner_idx: usize,
) -> Result<(), String> {
    terminal
        .draw(|frame| {
            let guard = display.lock().unwrap_or_else(|e| e.into_inner());
            render(frame, &guard, spinner_idx);
        })
        .map_err(|e| format!("TUI error: {}", e))
        .map(|_| ())
}

/// Run the view alongside `worker` until it finishes or the user cancels,
/// then print the final dump produced by `format`.
///
/// The outer error is an infrastructure failure (terminal error, worker
/// panic) that has not been shown to the user anywhere. The inner result is
/// the worker's own outcome, passed through untouched.
pub(crate) fn run_tui<W, E, R, D>(
    display: Arc<Mutex<Display>>,
    height: usize,
    worker: W,
    render: R,
    format: Option<D>,
    kill: Option<Arc<crate::exec::subprocess::RunKillSwitch>>,
) -> Result<Result<(), E>, String>
where
    W: FnOnce() -> Result<(), E> + Send + 'static,
    E: Send + 'static,
    R: Fn(&mut Frame, &Display, usize),
    D: Fn(&Display) -> String,
{
    signals::install_shutdown_handlers();

    let interactive = io::stdout().is_terminal();
    let keyboard = interactive && io::stdin().is_terminal();
    // Every display row is one terminal row.
    let height = height.min(u16::MAX as usize) as u16;

    // Raw mode comes first: the terminal's cursor-position query (used to
    // anchor the inline viewport) reads its response from stdin, which a
    // cooked terminal would not deliver.
    let _raw = if keyboard { RawMode::enable() } else { None };
    // A terminal that cannot answer the cursor-position query (dumb
    // terminals, some multiplexer harnesses) cannot host the inline view:
    // degrade to the headless path so the dump still prints instead of
    // failing the run.
    let mut terminal = if interactive {
        Terminal::with_options(
            CrosstermBackend::new(io::stdout()),
            TerminalOptions {
                viewport: Viewport::Inline(height.max(1)),
            },
        )
        .ok()
    } else {
        None
    };
    let live = terminal.is_some();

    let worker = std::thread::spawn(worker);
    let mut spinner_idx = 0;
    let mut cancelled = false;

    loop {
        if signals::cancel_requested() {
            cancelled = true;
            break;
        }
        if worker.is_finished() {
            break;
        }
        if keyboard && cancel_key_pressed() {
            cancelled = true;
            break;
        }
        if !keyboard {
            // Headless or key-less: nothing to poll, so pace the loop.
            std::thread::sleep(REDRAW_INTERVAL);
        }
        if let Some(terminal) = terminal.as_mut() {
            draw(terminal, &display, &render, spinner_idx)?;
            spinner_idx = (spinner_idx + 1) % SPINNER_FRAMES.len();
        }
    }

    // One last frame from the final state before leaving the inline view.
    if let Some(terminal) = terminal.as_mut() {
        draw(terminal, &display, &render, spinner_idx)?;
    }
    drop(terminal);
    drop(_raw);

    if cancelled {
        // Stop every live child process group so spawned commands cannot
        // outlive the run. The process ends here, so the escalation happens
        // synchronously: a polite SIGTERM first, then SIGKILL for whatever
        // is still alive. Joining the worker afterwards lets every
        // supervisor reap the child it owns instead of leaving zombies to
        // an init that may not reap them.
        if let Some(kill) = &kill {
            kill.kill_all();
            kill.kill_survivors();
        }
        let _ = worker.join();
        std::process::exit(130);
    }

    let worker_result: Result<(), E> = worker.join().map_err(|_| "worker panicked".to_string())?;

    if let Some(format) = format {
        let guard = display.lock().unwrap_or_else(|e| e.into_inner());
        let dump = format(&guard);
        drop(guard);
        let written = if live {
            dump_final_output(height, &dump)
        } else {
            write_dump(&dump)
        };
        written.map_err(|e| format!("failed to write output: {e}"))?;
    }
    Ok(worker_result)
}
