//! Validating the resolved graph: types, usage, and body and flow rules.
//!
//! A function declares its return type; a binding and a parameter declare
//! theirs; a literal is text, a record literal is its declared type, and a call
//! takes the type its callee declares. The check pass reads every body once and
//! enforces where each type may stand, recording what it derives in `Facts`;
//! the flow pass then settles which functions stop the run and enforces the
//! fall-through, break-placement and return-placement rules. Neither pass
//! writes a declaration or an expression.

mod declarations;
mod expression_typing;
mod flow;
mod flow_rules;
mod match_rules;
mod statement_check;

#[cfg(test)]
mod tests;

pub(crate) use declarations::validate_program;
