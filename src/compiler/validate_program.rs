//! Validating the resolved graph: types, usage, and body and flow rules.
//!
//! A function declares its return type; a binding and a parameter declare
//! theirs; a literal is text, a record literal is its declared type, and a call
//! takes the type its callee declares. The single validation walk then enforces
//! where each type may stand and the flow rules: `return` is an early exit
//! checked against the declared type, a value function must not fall through,
//! and a call to a function that stops the run ends its caller's path.

mod body_check;
mod body_expressions;
mod flow;
mod patterns;
mod validate_declarations;

#[cfg(test)]
mod tests;

pub(crate) use validate_declarations::validate_program;
