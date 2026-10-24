//! Pruning: keep only what the entry can reach.
//!
//! A compiled binary carries the resolved graph as its payload, so this stage
//! drops every declaration no running body reaches: an unused standard-library
//! helper, an unused user function, or a top-level value nothing reads. It
//! walks from the entry, marks every declaration a reached body calls,
//! references, or selects as a method target, then rewrites the graph to that
//! set with fresh `DeclarationId`s. Every edge is remapped, so a retained node
//! cannot point at a dropped one, the facts remap with the ids, the entry's new
//! id is returned, and the structural check the resolution stage runs is
//! repeated over the compacted graph.

mod declarations;
mod mark_reachable;

#[cfg(test)]
mod tests;

pub(crate) use declarations::prune_unreachable;
pub(crate) use mark_reachable::mark_reachable;
