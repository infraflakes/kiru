//! The execution IR: the compiler's only outward contract.
//!
//! Kiru compiles a `.kiru` config into an [`Ir`], a resolved, in-memory IR
//! that the executor consumes directly. The compiler also serializes `Ir`
//! to a textual "kirufile" (and parses it back) so the IR is debuggable and
//! inspectable: the format is RON, derived by serde, so the codec can never
//! drift from these type definitions. Everything is a resolved `String`:
//! there is no type or operator system, the DSL is an IaC task runner.

#[cfg(test)]
mod tests;
mod types;

pub(crate) use types::*;
