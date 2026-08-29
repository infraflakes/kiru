//! # Compiler Pipeline
//!
//! A single-pass eager pipeline. Parsing is a separate front-end that feeds the
//! compiler; runtime is a separate back-end that consumes its output.
//!
//! The compiler's output is a [`crate::plan::Plan`] — a fully lowered
//! configuration. Every top-level / project-body variable is frozen at compile
//! time (a `$(command)` part is evaluated once, at compile, and its stdout
//! captured). Function bodies are lowered to `Instruction`s and resolved at
//! runtime by the runner, where `$(command)` parts execute against a live scope
//! stack (local -> project -> global).

pub(crate) mod compile;
pub(crate) mod error;
pub(crate) mod validation;

/// Run the pipeline.
pub use compile::compile_and_resolve;
pub use error::CompileError;

#[cfg(test)]
pub(crate) mod test_support;
