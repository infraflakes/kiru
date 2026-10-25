//! The views: what a consumer asks of the graph.
//!
//! `compile` is the reachable set the payload is filtered to; `tokens` is the
//! role of every written name in one file. A view reads the graph and the
//! derived facts and owns no rule.

pub(crate) mod compile;
pub(crate) mod tokens;

pub(crate) use compile::{mark_reachable, verify_reachable};
