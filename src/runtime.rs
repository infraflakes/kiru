//! Running a compiled program: the run loop, threads, defers, and the process
//! kernel.

mod manage_processes;
mod run_program;

pub(crate) use run_program::run;

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

/// A failure that unwinds the body that raised it and runs its defers. It
/// leaves every other body running; the run exits nonzero once the entry body
/// ends and every thread has joined.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Panic;
