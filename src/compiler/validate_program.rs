//! Validating the resolved graph: kinds, usage, and body and flow rules.
//!
//! A function declares its return kind; a binding and a parameter declare
//! theirs; a literal is text, a record literal is a record, and a call takes
//! the kind its callee declares. The single validation walk then enforces
//! where each kind may stand and the flow rules: `return` is an early exit
//! checked against the declared kind, a value function must not fall through,
//! and a call to a function that stops the run ends its caller's path.

mod validate_bodies;
mod validate_declarations;

#[cfg(test)]
mod tests;

pub(crate) use validate_declarations::validate_program;
