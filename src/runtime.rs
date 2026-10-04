//! Running a compiled program: the run loop, threads, defers, and the process
//! kernel.

mod manage_processes;
mod run_program;

pub(crate) use run_program::run;

/// A failure that unwinds the body that raised it and runs its defers. It
/// leaves every other body running; the run exits nonzero once the entry body
/// ends and every thread has joined.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Panic;
