//! Checking the linked model: kinds, usage, and body rules.
//!
//! Every node's kind is derived bottom-up in declaration order. A keyword
//! fixes a binding and a parameter, a literal is text, a record literal is a
//! record, a call takes the callee's kind or its registry row's return, and
//! the runtime natives are fixed by their rows. The single validation walk
//! then enforces where each kind may stand and the structural rules:
//! `return(...)` closes a non-`main` function and is its last statement, and
//! `main` must not contain one.

mod declarations;
mod rules;

#[cfg(test)]
mod tests;

pub(crate) use declarations::analyze;
