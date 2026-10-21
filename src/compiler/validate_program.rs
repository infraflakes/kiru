//! Validating the resolved graph: types, usage, and body and flow rules.
//!
//! A function declares its return type; a binding and a parameter declare
//! theirs; a literal is text, a record literal is its declared type, and a call
//! takes the type its callee declares. The check pass types every body once and
//! enforces where each type may stand; the flow pass then settles which
//! functions stop the run and enforces the fall-through, break-placement and
//! return-placement rules.

mod expression_typing;
mod flow;
mod flow_check;
mod match_rules;
mod statement_check;
mod validate_declarations;

#[cfg(test)]
mod tests;

pub(crate) use validate_declarations::{validate_file, validate_program};
