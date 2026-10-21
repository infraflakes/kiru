//! A runtime failure and how it reaches the terminal.
//!
//! A failure unwinds the body that raised it, and the run reports it once at
//! the boundary. The marker and the rendering policy live here so no caller
//! invents its own way to stop or to print one.

/// A failure that unwinds the body that raised it. It
/// leaves every other body running; the run exits nonzero once the entry body
/// ends and every thread has joined.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Panic;

/// Report a runtime error the way `std::eprint` does: a red `ERROR:` prefix on
/// a terminal, then the message on stderr. The caller then stops the run, so
/// every failure a program can raise reads the same way.
pub(crate) fn report_error(message: &str) {
    use std::io::IsTerminal;
    if std::io::stderr().is_terminal() {
        eprintln!("\u{1b}[31mERROR:\u{1b}[0m {message}");
    } else {
        eprintln!("ERROR: {message}");
    }
}
