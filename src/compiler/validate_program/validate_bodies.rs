//! The validation walk over one declaration body: kinds, usage, and flow.
//!
//! The walk derives kinds bottom-up and returns the flow of every body; the
//! duplicate-pattern rule lives in `patterns`, and the flow algebra in `flow`.

mod flow;
mod patterns;
mod walk;

pub(super) use walk::Walk;
