//! The kiru compiler: its engine, runtime, syntax, command line, and the
//! language server's glue.
//!
//! One binary, `kc`, carries both faces: it compiles a program, and
//! `kc lsp` serves the language protocol from the same build. The compiler is
//! read through its build facade in `compiler::build`; everything under it
//! stays internal.

pub mod cli;
pub mod compiler;
pub mod lsp;
mod project;
pub mod runtime;
mod sync;
pub mod syntax;
