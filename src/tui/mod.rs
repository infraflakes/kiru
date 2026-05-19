use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{EnterAlternateScreen, LeaveAlternateScreen, disable_raw_mode, enable_raw_mode},
};
use ratatui::{Terminal, backend::CrosstermBackend};
use std::io;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::broadcast;
use tokio::sync::broadcast::Sender as BroadcastSender;

mod render;

pub fn send_event(tx: &BroadcastSender<TuiEvent>, event: TuiEvent) {
    if let Err(e) = tx.send(event) {
        eprintln!("[sro] warning: failed to send TUI event: {}", e);
    }
}

pub const SPINNER_FRAMES: &[char] = &['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];
pub const HEADER_HEIGHT: usize = 3;
pub const FOOTER_HEIGHT: usize = 2;
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
    pub expanded: bool,
}

impl Task {
    pub fn rendered_height(&self) -> usize {
        let mut height = 1; // task row
        if self.expanded && !self.output.is_empty() {
            let content_lines = self.output.len().min(MAX_PANEL_HEIGHT);
            let pruned = self.output.len() > MAX_PANEL_HEIGHT;
            height += content_lines + 1; // content + spacing
            if pruned {
                height += 1; // pruned indicator row
            }
        }
        height
    }
}

#[derive(Debug, Clone)]
pub struct Model {
    pub model_type: String,
    pub name: String,
    pub tasks: Vec<Task>,
    pub selected: usize,
    pub scroll_row: usize, // Scroll offset in rows, not task indices
}

impl Model {
    pub fn new(model_type: String, name: String) -> Self {
        Self {
            model_type,
            name,
            tasks: Vec::new(),
            selected: 0,
            scroll_row: 0,
        }
    }

    pub fn add_task(&mut self, name: String) {
        self.tasks.push(Task {
            name,
            status: TaskStatus::Pending,
            output: Vec::new(),
            expanded: false,
        });
    }

    pub fn update_task_status(&mut self, index: usize, status: TaskStatus) {
        if let Some(task) = self.tasks.get_mut(index) {
            task.status = status;
            if status == TaskStatus::Running {
                task.expanded = true;
            }
        }
    }

    pub fn append_output(&mut self, idx: usize, line: String) {
        if idx < self.tasks.len() {
            self.tasks[idx].output.push(line);
        }
    }

    pub fn task_y_position(&self, index: usize) -> usize {
        let mut y = 0;
        for (i, task) in self.tasks.iter().enumerate() {
            if i == index {
                break;
            }
            y += task.rendered_height();
        }
        y
    }

    pub fn scroll_to_visible(&mut self, terminal_height: u16) {
        let max_y =
            terminal_height.saturating_sub(HEADER_HEIGHT as u16 + FOOTER_HEIGHT as u16) as usize;
        let task_y = self.task_y_position(self.selected);
        let task_height = self.tasks[self.selected].rendered_height();

        // If task is outside visible area, scroll to snap to task start
        if task_y < self.scroll_row || task_y + task_height > self.scroll_row + max_y {
            self.scroll_row = task_y;
        }
    }
}

struct TerminalGuard {
    terminal: Option<Terminal<CrosstermBackend<io::Stdout>>>,
}

impl TerminalGuard {
    fn new() -> Result<Self, io::Error> {
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;
        terminal.clear()?;
        Ok(Self {
            terminal: Some(terminal),
        })
    }

    fn terminal(&mut self) -> &mut Terminal<CrosstermBackend<io::Stdout>> {
        self.terminal.as_mut().unwrap()
    }
}

impl Drop for TerminalGuard {
    fn drop(&mut self) {
        if let Some(ref mut terminal) = self.terminal {
            let _ = disable_raw_mode();
            let _ = execute!(
                terminal.backend_mut(),
                LeaveAlternateScreen,
                DisableMouseCapture
            );
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
        let mut guard = TerminalGuard::new()?;
        let terminal = guard.terminal();

        let mut rx = self.rx.lock().unwrap().take().expect("run() called twice");
        let mut spinner_idx = 0;

        loop {
            // Drain all pending events — handle Lagged to avoid losing status updates
            loop {
                match rx.try_recv() {
                    Ok(event) => match event {
                        TuiEvent::UpdateStatus(idx, status) => {
                            let mut model = self.model.lock().unwrap();
                            model.update_task_status(idx, status);
                        }
                        TuiEvent::AppendOutput(idx, line) => {
                            let mut model = self.model.lock().unwrap();
                            model.append_output(idx, line);
                        }
                    },
                    Err(broadcast::error::TryRecvError::Lagged(_)) => continue,
                    Err(_) => break,
                }
            }

            // Handle keyboard events
            if event::poll(Duration::from_millis(50))?
                && let Event::Key(key) = event::read()?
            {
                match key.code {
                    KeyCode::Char('q') => break,
                    KeyCode::Down => {
                        let mut model = self.model.lock().unwrap();
                        if model.selected < model.tasks.len().saturating_sub(1) {
                            model.selected += 1;
                            let size = terminal.size()?;
                            model.scroll_to_visible(size.height);
                        }
                    }
                    KeyCode::Up => {
                        let mut model = self.model.lock().unwrap();
                        if model.selected > 0 {
                            model.selected -= 1;
                            let size = terminal.size()?;
                            model.scroll_to_visible(size.height);
                        }
                    }
                    KeyCode::Enter | KeyCode::Char(' ') | KeyCode::Right => {
                        let selected = {
                            let model = self.model.lock().unwrap();
                            model.selected
                        };
                        let mut model = self.model.lock().unwrap();
                        if let Some(task) = model.tasks.get_mut(selected) {
                            task.expanded = !task.expanded;
                            let size = terminal.size()?;
                            model.scroll_to_visible(size.height);
                        }
                    }
                    KeyCode::Left => {
                        let selected = {
                            let model = self.model.lock().unwrap();
                            model.selected
                        };
                        let mut model = self.model.lock().unwrap();
                        if let Some(task) = model.tasks.get_mut(selected)
                            && task.expanded
                        {
                            task.expanded = false;
                        }
                    }
                    _ => {}
                }
            }

            // Render
            spinner_idx = (spinner_idx + 1) % SPINNER_FRAMES.len();
            terminal.draw(|f| {
                let model = self.model.lock().unwrap();
                render::render(f, &model, spinner_idx);
            })?;
        }
        Ok(())
    }
}
