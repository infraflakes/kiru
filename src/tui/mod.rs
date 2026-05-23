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
use std::io::{self, Write};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::broadcast;
use tokio::sync::broadcast::Sender as BroadcastSender;

/// A wrapper around [`CrosstermBackend`] that returns fallback dimensions (80x24) if the
/// terminal size query fails. This allows the TUI to function in non-TTY environments
/// where `ioctl(TIOCGWINSZ)` would return "No such device or address".
struct SafeBackend<W: Write> {
    inner: CrosstermBackend<W>,
}

impl<W: Write> SafeBackend<W> {
    const fn new(inner: CrosstermBackend<W>) -> Self {
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

mod render;

pub fn send_event(tx: &BroadcastSender<TuiEvent>, event: TuiEvent) {
    if let Err(e) = tx.send(event) {
        eprintln!("[sro] warning: failed to send TUI event: {}", e);
    }
}

pub const SPINNER_FRAMES: &[char] = &['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];
pub const MAX_PANEL_HEIGHT: usize = 15;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TaskStatus {
    Pending,
    Running,
    Success,
    Error,
}

#[derive(Debug, Clone)]
pub struct Task {
    pub name: String,
    pub status: TaskStatus,
    pub output: Vec<String>,
    pub finalized: bool,
}

#[derive(Debug, Clone)]
pub struct Model {
    #[allow(dead_code)]
    pub model_type: String,
    #[allow(dead_code)]
    pub name: String,
    pub tasks: Vec<Task>,
    #[allow(dead_code)]
    pub selected: usize,
}

impl Model {
    pub fn new(model_type: String, name: String) -> Self {
        Self {
            model_type,
            name,
            tasks: Vec::new(),
            selected: 0,
        }
    }

    pub fn add_task(&mut self, name: String) {
        self.tasks.push(Task {
            name,
            status: TaskStatus::Pending,
            output: Vec::new(),
            finalized: false,
        });
    }

    pub fn update_task_status(&mut self, index: usize, status: TaskStatus) {
        if let Some(task) = self.tasks.get_mut(index) {
            task.status = status;
            task.finalized = matches!(status, TaskStatus::Success | TaskStatus::Error);
        }
    }

    pub fn append_output(&mut self, idx: usize, line: String) {
        if idx < self.tasks.len() {
            self.tasks[idx].output.push(line);
        }
    }

    pub fn all_done(&self) -> bool {
        self.tasks
            .iter()
            .all(|t| matches!(t.status, TaskStatus::Success | TaskStatus::Error))
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

pub struct TuiApp {
    model: Arc<Mutex<Model>>,
    tx: broadcast::Sender<TuiEvent>,
    rx: Mutex<Option<broadcast::Receiver<TuiEvent>>>,
}

#[derive(Debug, Clone)]
pub enum TuiEvent {
    UpdateStatus(usize, TaskStatus),
    AppendOutput(usize, String),
}

impl TuiApp {
    pub fn new(model: Model) -> Self {
        let model = Arc::new(Mutex::new(model));
        let (tx, rx) = broadcast::channel(10000);
        Self {
            model,
            tx,
            rx: Mutex::new(Some(rx)),
        }
    }

    pub fn get_sender(&self) -> broadcast::Sender<TuiEvent> {
        self.tx.clone()
    }

    pub async fn run(&self) -> Result<(), io::Error> {
        let raw = RawMode::try_enable();

        let mut terminal = Terminal::with_options(
            SafeBackend::new(CrosstermBackend::new(io::stdout())),
            TerminalOptions {
                viewport: Viewport::Inline(1),
            },
        )?;

        let mut rx = self.rx.lock().unwrap().take().expect("run() called twice");
        let mut spinner_idx = 0;

        loop {
            loop {
                match rx.try_recv() {
                    Ok(TuiEvent::UpdateStatus(idx, status)) => {
                        self.model.lock().unwrap().update_task_status(idx, status);
                    }
                    Ok(TuiEvent::AppendOutput(idx, line)) => {
                        self.model.lock().unwrap().append_output(idx, line);
                    }
                    Err(broadcast::error::TryRecvError::Lagged(_)) => continue,
                    Err(_) => break,
                }
            }

            if self.model.lock().unwrap().all_done() {
                terminal.draw(|f| {
                    let model = self.model.lock().unwrap();
                    render::render(f, &model, spinner_idx);
                })?;
                break;
            }

            if matches!(raw, RawMode::Enabled) {
                if event::poll(Duration::from_millis(50))?
                    && let Event::Key(key) = event::read()?
                {
                    match key.code {
                        KeyCode::Char('q') => break,
                        KeyCode::Char('c') if key.modifiers.contains(KeyModifiers::CONTROL) => {
                            break;
                        }
                        _ => {}
                    }
                }
            } else {
                tokio::time::sleep(Duration::from_millis(50)).await;
            }

            spinner_idx = (spinner_idx + 1) % SPINNER_FRAMES.len();
            terminal.draw(|f| {
                let model = self.model.lock().unwrap();
                render::render(f, &model, spinner_idx);
            })?;
        }

        drop(terminal);
        drop(raw);

        {
            let model = self.model.lock().unwrap();
            let mut out = io::stdout().lock();
            render::dump_final(&model, &mut out)?;
        }

        Ok(())
    }
}
