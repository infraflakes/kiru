//! Running a compiled program: the run loop, threads, defers, and the process
//! kernel.

mod natives;
mod processes;
mod virtual_machine;

pub(crate) use virtual_machine::run;

/// The name this program reports under: the invoked binary's file name, or
/// `kiru` when the name cannot be read. The compiler reports under `kc`
/// instead; a compiled program reports under whatever the user named it.
pub(crate) fn program_name() -> String {
    std::env::args_os()
        .next()
        .and_then(|argument| {
            std::path::Path::new(&argument)
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
        })
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "kiru".to_owned())
}

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

/// A failure that unwinds the body that raised it and runs its defers. It
/// leaves every other body running; the run exits nonzero once the entry body
/// ends and every thread has joined.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Panic;
