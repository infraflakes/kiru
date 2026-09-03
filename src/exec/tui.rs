use crossterm::{
    event::{self, Event, KeyCode, KeyModifiers},
    terminal::{disable_raw_mode, enable_raw_mode},
};

use ratatui::{Frame, Terminal, TerminalOptions, Viewport, backend::CrosstermBackend};
use std::future::Future;
use std::io::{self, Write};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::mpsc;

mod crossterm_backend;
pub(super) mod model;
pub(super) mod render;
pub(super) mod run;
pub(super) mod sync;

use model::{Model, TaskRow, TaskStatus};

use crossterm_backend::SafeBackend;

/// Send a TuiEvent over the channel. Ok to fail, receiver may have disconnected.
pub(crate) fn send_tui_event(sender: &mpsc::UnboundedSender<TuiEvent>, event: TuiEvent) {
    let _ = sender.send(event);
}

const SPINNER_FRAMES: &[char] = &['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];
pub(super) const MAX_PANEL_HEIGHT: usize = 15;

enum RawMode {
    Enabled,
    Unsupported,
}

impl RawMode {
    fn try_enable() -> Self {
        match enable_raw_mode() {
            Ok(()) => RawMode::Enabled,
            Err(_) => RawMode::Unsupported,
        }
    }
}

impl Drop for RawMode {
    fn drop(&mut self) {
        if let RawMode::Enabled = self {
            let _ = disable_raw_mode();
        }
    }
}

/// Events sent from worker threads to the TUI event loop.
///
/// - `UpdateStatus(i, s)`, set task at index `i` to status `s`.
/// - `AppendOutput(i, line)`, append a line of output to task `i`.
#[derive(Debug, Clone)]
pub(crate) enum TuiEvent {
    UpdateStatus(usize, TaskStatus),
    AppendOutput(usize, String),
}

/// Drain all available events from the channel, updating the model.
/// Returns `true` if the sender has disconnected.
fn drain_events(
    model: &Arc<Mutex<Model>>,
    event_receiver: &mut mpsc::UnboundedReceiver<TuiEvent>,
) -> bool {
    loop {
        match event_receiver.try_recv() {
            Ok(TuiEvent::UpdateStatus(idx, status)) => {
                model
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .update_task_status(idx, status);
            }
            Ok(TuiEvent::AppendOutput(idx, line)) => {
                model
                    .lock()
                    .unwrap_or_else(|e| e.into_inner())
                    .append_output(idx, line);
            }
            Err(mpsc::error::TryRecvError::Empty) => return false,
            Err(mpsc::error::TryRecvError::Disconnected) => return true,
        }
    }
}

/// Poll for keyboard input.  Returns `true` when the user requested exit.
fn handle_keyboard_input() -> bool {
    if matches!(event::poll(Duration::from_millis(50)), Ok(true))
        && let Ok(Event::Key(key)) = event::read()
    {
        matches!(key.code, KeyCode::Char('q'))
            || (key.code == KeyCode::Char('c') && key.modifiers.contains(KeyModifiers::CONTROL))
    } else {
        false
    }
}

/// Scroll past the TUI viewport and write the final formatted output to stdout.
fn dump_final_output(height: u16, dump: &str) -> Result<(), io::Error> {
    let mut out = io::stdout().lock();
    for _ in 0..height {
        out.write_all(b"\n")?;
    }
    out.flush()?;
    let mut out = io::stdout().lock();
    out.write_all(dump.as_bytes())?;
    out.flush()?;
    Ok(())
}

/// Main TUI event loop: drains events from the channel, draws frames, and
/// handles keyboard input (q / Ctrl+C to quit).
/// Returns `true` if the user cancelled via keyboard, `false` if the worker
/// finished naturally.
pub(crate) async fn run_tui_event_loop(
    model: Arc<Mutex<Model>>,
    mut event_receiver: mpsc::UnboundedReceiver<TuiEvent>,
    height: u16,
    render_fn: fn(&mut Frame, &Model, usize),
    format_fn: Option<fn(&Model) -> String>,
) -> Result<bool, io::Error> {
    let raw = RawMode::try_enable();

    let mut terminal = Terminal::with_options(
        SafeBackend::new(CrosstermBackend::new(io::stdout())),
        TerminalOptions {
            viewport: Viewport::Inline(height.max(1)),
        },
    )?;

    let mut spinner_idx = 0;
    let mut cancelled = false;

    loop {
        let disconnected = drain_events(&model, &mut event_receiver);

        if disconnected || model.lock().unwrap_or_else(|e| e.into_inner()).all_done() {
            terminal.draw(|frame| {
                let guard = model.lock().unwrap_or_else(|e| e.into_inner());
                render_fn(frame, &guard, spinner_idx);
            })?;
            break;
        }

        if matches!(raw, RawMode::Enabled) {
            if handle_keyboard_input() {
                let _ = disable_raw_mode();
                cancelled = true;
                break;
            }
        } else {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }

        spinner_idx = (spinner_idx + 1) % SPINNER_FRAMES.len();
        terminal.draw(|frame| {
            let guard = model.lock().unwrap_or_else(|e| e.into_inner());
            render_fn(frame, &guard, spinner_idx);
        })?;
    }

    drop(terminal);
    drop(raw);

    let guard = model.lock().unwrap_or_else(|e| e.into_inner());
    let dump = format_fn.map(|format_fn| format_fn(&guard));
    drop(guard);

    if let Some(dump) = dump {
        dump_final_output(height, &dump)?;
    }
    Ok(cancelled)
}

/// Set up the tokio runtime, build the model from chains, and run the TUI
/// alongside the given worker future. `format_fn` produces the final text
/// dump after the TUI closes; `None` skips the dump entirely (e.g. sync).
///
/// The outer error is an infrastructure failure (TUI panic, terminal error,
/// worker panic) that has not been shown to the user anywhere. The inner
/// result is the worker's own outcome, passed through untouched.
pub(crate) fn run_tui_with<F, Fut, E>(
    chains: Vec<(String, Vec<String>)>,
    worker: F,
    render_fn: fn(&mut Frame, &Model, usize),
    format_fn: Option<fn(&Model) -> String>,
) -> Result<Result<(), E>, String>
where
    F: FnOnce(mpsc::UnboundedSender<TuiEvent>) -> Fut + Send + 'static,
    Fut: Future<Output = Result<(), E>> + Send + 'static,
    E: Send + 'static,
{
    let tokio_runtime = tokio::runtime::Runtime::new().map_err(|e| format!("{}", e))?;
    tokio_runtime.block_on(async {
        let mut model = Model::new();
        for (label, task_names) in chains {
            model.add_chain(label, task_names);
        }
        let height: u16 = model
            .chains
            .iter()
            .map(|c| 1u16.saturating_add(c.task_count as u16))
            .try_fold(0u16, |acc, h| acc.checked_add(h))
            .unwrap_or(u16::MAX);
        let model = Arc::new(Mutex::new(model));
        let (event_sender, event_receiver) = mpsc::unbounded_channel();
        let tui = tokio::spawn(run_tui_event_loop(
            model,
            event_receiver,
            height,
            render_fn,
            format_fn,
        ));
        let worker = tokio::spawn(worker(event_sender));

        let cancelled = tui
            .await
            .map_err(|e| format!("TUI panicked: {}", e))?
            .map_err(|e| format!("TUI error: {}", e))?;

        if cancelled {
            // Kill everything immediately, running shell commands included.
            // The TUI loop already disabled raw mode and dropped the terminal.
            std::process::exit(130);
        }

        let worker_result: Result<(), E> = worker
            .await
            .map_err(|e| format!("worker panicked: {}", e))?;
        Ok(worker_result)
    })
}
