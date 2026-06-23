use crossterm::{
    event::{self, Event, KeyCode, KeyModifiers},
    terminal::{disable_raw_mode, enable_raw_mode},
};

use ratatui::{
    Terminal, TerminalOptions, Viewport,
    backend::{Backend, ClearType, CrosstermBackend, WindowSize},
    buffer::Cell,
    layout::{Position, Size},
};
use std::future::Future;
use std::io::{self, Write};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::mpsc;

/// A wrapper around [`CrosstermBackend`] that returns fallback dimensions (80x24) if the
/// terminal size query fails. This allows the TUI to function in non-TTY environments
/// where `ioctl(TIOCGWINSZ)` would return "No such device or address".
struct SafeBackend<W: Write> {
    inner: CrosstermBackend<W>,
}

impl<W: Write> SafeBackend<W> {
    fn new(inner: CrosstermBackend<W>) -> Self {
        Self { inner }
    }
}

impl<W: Write> Write for SafeBackend<W> {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.inner.write(buf)
    }
    fn flush(&mut self) -> io::Result<()> {
        io::Write::flush(&mut self.inner)
    }
}

impl<W: Write> Backend for SafeBackend<W> {
    type Error = io::Error;

    fn draw<'a, I>(&mut self, content: I) -> Result<(), Self::Error>
    where
        I: Iterator<Item = (u16, u16, &'a Cell)>,
    {
        self.inner.draw(content)
    }

    fn hide_cursor(&mut self) -> Result<(), Self::Error> {
        self.inner.hide_cursor()
    }

    fn show_cursor(&mut self) -> Result<(), Self::Error> {
        self.inner.show_cursor()
    }

    fn get_cursor_position(&mut self) -> Result<Position, Self::Error> {
        match self.inner.get_cursor_position() {
            Ok(pos) => Ok(pos),
            Err(_) => Ok(Position { x: 0, y: 0 }),
        }
    }

    fn set_cursor_position<P: Into<Position>>(&mut self, position: P) -> Result<(), Self::Error> {
        self.inner.set_cursor_position(position)
    }

    fn clear(&mut self) -> Result<(), Self::Error> {
        self.inner.clear()
    }

    fn clear_region(&mut self, clear_type: ClearType) -> Result<(), Self::Error> {
        self.inner.clear_region(clear_type)
    }

    fn append_lines(&mut self, n: u16) -> Result<(), Self::Error> {
        self.inner.append_lines(n)
    }

    fn size(&self) -> Result<Size, Self::Error> {
        match self.inner.size() {
            Ok(size) => Ok(size),
            Err(_) => Ok(Size {
                width: 80,
                height: 24,
            }),
        }
    }

    fn window_size(&mut self) -> Result<WindowSize, Self::Error> {
        match self.inner.window_size() {
            Ok(ws) => Ok(ws),
            Err(_) => Ok(WindowSize {
                columns_rows: Size {
                    width: 80,
                    height: 24,
                },
                pixels: Size {
                    width: 0,
                    height: 0,
                },
            }),
        }
    }

    fn flush(&mut self) -> Result<(), Self::Error> {
        Backend::flush(&mut self.inner)
    }
}

pub mod render;

pub fn send_event(tx: &mpsc::UnboundedSender<TuiEvent>, event: TuiEvent) {
    if tx.send(event).is_err() {
        eprintln!("[kiru] warning: failed to send TUI event");
    }
}

const SPINNER_FRAMES: &[char] = &['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];
pub(super) const MAX_PANEL_HEIGHT: usize = 15;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskStatus {
    Pending,
    Running,
    Success,
    Error,
    Skipped,
}

#[derive(Debug, Clone)]
pub(super) struct Task {
    pub name: String,
    pub status: TaskStatus,
    pub output: Vec<String>,
    pub finalized: bool,
}

#[derive(Debug, Clone)]
pub(super) struct Chain {
    pub label: String,
    pub task_start: usize,
    pub task_count: usize,
}

#[derive(Debug, Clone)]
pub struct Model {
    pub tasks: Vec<Task>,
    pub chains: Vec<Chain>,
}

impl Model {
    pub fn new() -> Self {
        Self {
            tasks: Vec::new(),
            chains: Vec::new(),
        }
    }

    fn lock(arc: &Arc<Mutex<Model>>) -> std::sync::MutexGuard<'_, Model> {
        arc.lock().unwrap_or_else(|e| e.into_inner())
    }

    pub fn add_chain(&mut self, label: String, task_names: Vec<String>) {
        let task_start = self.tasks.len();
        let task_count = task_names.len();
        for name in task_names {
            self.tasks.push(Task {
                name,
                status: TaskStatus::Pending,
                output: Vec::new(),
                finalized: false,
            });
        }
        self.chains.push(Chain {
            label,
            task_start,
            task_count,
        });
    }

    pub fn update_task_status(&mut self, index: usize, status: TaskStatus) {
        if let Some(task) = self.tasks.get_mut(index) {
            task.status = status;
            task.finalized = matches!(
                status,
                TaskStatus::Success | TaskStatus::Error | TaskStatus::Skipped
            );
        }
    }

    pub fn append_output(&mut self, idx: usize, line: String) {
        if idx < self.tasks.len() {
            self.tasks[idx].output.push(line);
        }
    }

    pub fn all_done(&self) -> bool {
        self.tasks.iter().all(|t| {
            matches!(
                t.status,
                TaskStatus::Success | TaskStatus::Error | TaskStatus::Skipped
            )
        })
    }

    pub fn chain_status(&self, chain: &Chain) -> TaskStatus {
        let mut has_error = false;
        let mut has_running = false;
        let mut has_pending = false;
        for i in chain.task_start..chain.task_start + chain.task_count {
            if let Some(task) = self.tasks.get(i) {
                match task.status {
                    TaskStatus::Error => has_error = true,
                    TaskStatus::Running => has_running = true,
                    TaskStatus::Pending => has_pending = true,
                    _ => {}
                }
            }
        }
        if has_error {
            TaskStatus::Error
        } else if has_running {
            TaskStatus::Running
        } else if has_pending {
            TaskStatus::Pending
        } else {
            TaskStatus::Success
        }
    }
}

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

pub async fn run_tui(
    model: Arc<Mutex<Model>>,
    mut rx: mpsc::UnboundedReceiver<TuiEvent>,
    height: u16,
) -> Result<(), io::Error> {
    let raw = RawMode::try_enable();

    let mut terminal = Terminal::with_options(
        SafeBackend::new(CrosstermBackend::new(io::stdout())),
        TerminalOptions {
            viewport: Viewport::Inline(height.max(1)),
        },
    )?;

    let mut spinner_idx = 0;

    loop {
        loop {
            match rx.try_recv() {
                Ok(TuiEvent::UpdateStatus(idx, status)) => {
                    Model::lock(&model).update_task_status(idx, status);
                }
                Ok(TuiEvent::AppendOutput(idx, line)) => {
                    Model::lock(&model).append_output(idx, line);
                }
                Err(mpsc::error::TryRecvError::Empty) => break,
                Err(mpsc::error::TryRecvError::Disconnected) => break,
            }
        }

        if Model::lock(&model).all_done() {
            terminal.draw(|f| {
                let guard = Model::lock(&model);
                render::render(f, &guard, spinner_idx);
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
        terminal.draw(|f| {
            let guard = Model::lock(&model);
            render::render(f, &guard, spinner_idx);
        })?;
    }

    drop(terminal);
    drop(raw);

    // scroll past the TUI viewport so the ANSI dump doesn't overlay
    // leftover TUI content on the terminal (which caused visual corruption
    // like " ✓ fmt(kiru)u)  ✗ failed")
    {
        let mut out = io::stdout().lock();
        for _ in 0..height {
            out.write_all(b"\n")?;
        }
        out.flush()?;
    }

    let guard = Model::lock(&model);
    let dump = render::format_final_output(&guard);
    drop(guard);
    let mut out = io::stdout().lock();
    out.write_all(dump.as_bytes())?;
    out.flush()?;
    Ok(())
}

pub(crate) fn run_tui_with<F, Fut>(
    chains: Vec<(String, Vec<String>)>,
    worker: F,
) -> miette::Result<()>
where
    F: FnOnce(mpsc::UnboundedSender<TuiEvent>) -> Fut + Send + 'static,
    Fut: Future<Output = miette::Result<()>> + Send + 'static,
{
    let rt = tokio::runtime::Runtime::new().map_err(|e| miette::miette!("{}", e))?;
    rt.block_on(async {
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
        let (tx, rx) = mpsc::unbounded_channel();
        let tui = tokio::spawn(run_tui(model, rx, height));
        let worker = tokio::spawn(worker(tx));

        tui.await
            .map_err(|e| miette::miette!("TUI panicked: {}", e))?
            .map_err(|e| miette::miette!("TUI error: {}", e))?;
        worker
            .await
            .map_err(|e| miette::miette!("worker panicked: {}", e))?
    })
}
