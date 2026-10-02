//! Running a compiled program: the interpreter, threads, defers, and the
//! process kernel.

mod interpreter;
mod process;

#[cfg(test)]
pub(crate) use interpreter::Runtime;
pub(crate) use interpreter::run;

/// A failure that unwinds the body that raised it and runs its defers. It
/// leaves every other body running; the run exits nonzero once the entry body
/// ends and every thread has joined.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Panic;
