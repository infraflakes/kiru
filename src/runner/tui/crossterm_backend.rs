use ratatui::{
    backend::{Backend, ClearType, CrosstermBackend, WindowSize},
    buffer::Cell,
    layout::{Position, Size},
};
use std::io::{self, Write};

/// A wrapper around [`CrosstermBackend`] that returns fallback dimensions (80x24) if the
/// terminal size query fails. This allows the TUI to function in non-TTY environments
/// where `ioctl(TIOCGWINSZ)` would return "No such device or address".
pub(super) struct SafeBackend<W: Write> {
    inner: CrosstermBackend<W>,
}

impl<W: Write> SafeBackend<W> {
    pub(super) fn new(inner: CrosstermBackend<W>) -> Self {
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
            Ok(window_size) => Ok(window_size),
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
