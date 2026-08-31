//! The execution IR: the compiler's only outward contract.
//!
//! Kiru lowers a `.kiru` config into an [`Ir`], a resolved, in-memory IR that
//! the executor consumes directly. The compiler also serializes `Ir` to a
//! textual "kirufile" s-expression (and parses it back) so the IR is
//! debuggable and inspectable. Everything is a resolved `String`: there is no
//! type or operator system, the DSL is an IaC task runner.

mod deserialize;
mod serialize;
#[cfg(test)]
mod tests;
mod types;

pub(crate) use serialize::*;
pub(crate) use types::*;
