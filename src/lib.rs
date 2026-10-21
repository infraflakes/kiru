//! The kiru compiler: its engine, runtime, syntax, command line, and the
//! language server's glue.
//!
//! The two binaries share this library: `kc`, the compiler, and `kls`, the
//! language server. The compiler is read through its build facade in
//! `compiler::build`; everything under it stays internal.

pub mod cli;
pub mod compiler;
pub mod lsp;
pub mod runtime;
pub mod syntax;
