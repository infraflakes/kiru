//! Resolving names: build the program graph from parsed files.
//!
//! This stage registers every declaration into its namespace, resolves every
//! name into an edge to a node, and selects the entry: the entry file's own
//! root `main`. After this stage, no later stage resolves a name again.

mod check_invariants;
mod link_bodies;
mod register_declarations;

#[cfg(test)]
mod tests;

pub(crate) use check_invariants::verify;
pub(crate) use link_bodies::resolve_names;
