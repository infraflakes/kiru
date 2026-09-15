//! Pruning: keep only what the entry can reach.
//!
//! A compiled binary carries the resolved graph as its payload, so this stage
//! drops every declaration no running body reaches: an unused standard-library
//! helper, an unused user function, or a top-level value nothing reads. It
//! walks from `Program.entry`, marks every declaration a reached body calls or
//! references, then rewrites the graph to that set with fresh `DeclarationId`s.
//! Every edge is remapped, so a retained node cannot point at a dropped one,
//! and the structural check the resolution stage runs is repeated over the
//! compacted graph.

mod mark_reachable;
mod prune_declarations;

#[cfg(test)]
mod tests;

pub(crate) use prune_declarations::prune_unreachable;
