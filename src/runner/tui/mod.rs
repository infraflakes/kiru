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
pub(crate) mod render;
pub(crate) mod run;
pub(crate) mod sync;

pub(crate) use model::{Model, Task, TaskStatus};

use crossterm_backend::SafeBackend;

pub fn send_tui_event(sender: &mpsc::UnboundedSender<TuiEvent>, event: TuiEvent) {
    if sender.send(event).is_err() {
        eprintln!("[kiru] warning: failed to send TUI event");
    }
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

#[derive(Debug, Clone)]
pub enum TuiEvent {
    UpdateStatus(usize, TaskStatus),
    AppendOutput(usize, String),
}

pub async fn run_tui_event_loop(
    model: Arc<Mutex<Model>>,
    mut event_receiver: mpsc::UnboundedReceiver<TuiEvent>,
    height: u16,
    render_fn: fn(&mut Frame, &Model, usize),
    format_fn: fn(&Model) -> String,
) -> Result<(), io::Error> {
    let raw = RawMode::try_enable();

    let mut terminal = Terminal::with_options(
        SafeBackend::new(CrosstermBackend::new(io::stdout())),
        TerminalOptions {
            viewport: Viewport::Inline(height.max(1)),
        },
    )?;

    let mut spinner_idx = 0;
    let mut disconnected = false;

    loop {
        loop {
            match event_receiver.try_recv() {
                Ok(TuiEvent::UpdateStatus(idx, status)) => {
                    Model::lock(&model).update_task_status(idx, status);
                }
                Ok(TuiEvent::AppendOutput(idx, line)) => {
                    Model::lock(&model).append_output(idx, line);
                }
                Err(mpsc::error::TryRecvError::Empty) => break,
                Err(mpsc::error::TryRecvError::Disconnected) => {
                    disconnected = true;
                    break;
                }
            }
        }

        if disconnected || Model::lock(&model).all_done() {
            terminal.draw(|frame| {
                let guard = Model::lock(&model);
                render_fn(frame, &guard, spinner_idx);
            })?;
            break;
        }

        if matches!(raw, RawMode::Enabled) {
            if event::poll(Duration::from_millis(50))?
                && let Event::Key(key) = event::read()?
            {
                match key.code {
                    KeyCode::Char('q') => break,
                    KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => break,
                    _ => {}
                }
            }
        } else {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }

        spinner_idx = (spinner_idx + 1) % SPINNER_FRAMES.len();
        terminal.draw(|frame| {
            let guard = Model::lock(&model);
            render_fn(frame, &guard, spinner_idx);
        })?;
    }

    drop(terminal);
    drop(raw);

    let guard = Model::lock(&model);
    let dump = format_fn(&guard);
    drop(guard);

    if !dump.is_empty() {
        // scroll past the TUI viewport so the ANSI dump doesn't overlay
        // leftover TUI content on the terminal (which caused visual corruption
        // like " ✓ fmt(kiru)u)  ✗ failed")
        let mut out = io::stdout().lock();
        for _ in 0..height {
            out.write_all(b"\n")?;
        }
        out.flush()?;

        let mut out = io::stdout().lock();
        out.write_all(dump.as_bytes())?;
        out.flush()?;
    }
    Ok(())
}

pub(crate) fn run_tui_with<F, Fut>(
    chains: Vec<(String, Vec<String>)>,
    worker: F,
    render_fn: fn(&mut Frame, &Model, usize),
    format_fn: fn(&Model) -> String,
) -> miette::Result<()>
where
    F: FnOnce(mpsc::UnboundedSender<TuiEvent>) -> Fut + Send + 'static,
    Fut: Future<Output = miette::Result<()>> + Send + 'static,
{
    let tokio_runtime = tokio::runtime::Runtime::new().map_err(|e| miette::miette!("{}", e))?;
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
        let tui = tokio::spawn(run_tui_event_loop(model, event_receiver, height, render_fn, format_fn));
        let worker = tokio::spawn(worker(event_sender));

        tui.await
            .map_err(|e| miette::miette!("TUI panicked: {}", e))?
            .map_err(|e| miette::miette!("TUI error: {}", e))?;
        worker
            .await
            .map_err(|e| miette::miette!("worker panicked: {}", e))?
    })
}

pub(crate) fn run_tui_with_run<F, Fut>(
    chains: Vec<(String, Vec<String>)>,
    worker: F,
) -> miette::Result<()>
where
    F: FnOnce(mpsc::UnboundedSender<TuiEvent>) -> Fut + Send + 'static,
    Fut: Future<Output = miette::Result<()>> + Send + 'static,
{
    run_tui_with(chains, worker, run::render_run_output, run::format_final_output)
}

pub(crate) fn run_tui_with_sync<F, Fut>(
    chains: Vec<(String, Vec<String>)>,
    worker: F,
) -> miette::Result<()>
where
    F: FnOnce(mpsc::UnboundedSender<TuiEvent>) -> Fut + Send + 'static,
    Fut: Future<Output = miette::Result<()>> + Send + 'static,
{
    run_tui_with(chains, worker, sync::render_sync_output, |_| String::new())
}
