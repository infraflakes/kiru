//! Validating the resolved graph: kinds, usage, and body and flow rules.
//!
//! Every node's kind is derived bottom-up in declaration order. A keyword
//! fixes a binding and a parameter, a literal is text, a record literal is a
//! record, and a call takes the callee's kind. The single validation walk
//! then enforces where each kind may stand and the structural rules: `return`
//! is an early exit, a value function must not fall through, and `panic;` and
//! `return` are terminators.

mod validate_bodies;
mod validate_declarations;

#[cfg(test)]
mod tests;

pub(crate) use validate_declarations::validate_program;
