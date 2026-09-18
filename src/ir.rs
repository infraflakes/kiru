//! The execution IR: the compiler's only outward contract.
//!
//! Kiru compiles a `.kiru` config into a [`Program`], a resolved arena of
//! statement nodes that the executor consumes directly and the renderers
//! walk. The compiler also serializes `Program` to the profile's output
//! (and parses it back) so the compiled form is debuggable and inspectable:
//! the format is RON, derived by serde, so the codec can never drift from
//! these type definitions. Everything is a resolved `String` or a [`Template`]: there
//! is no type or operator system, the DSL is an IaC task runner.

#[cfg(test)]
mod tests;
mod types;

pub(crate) use types::*;
