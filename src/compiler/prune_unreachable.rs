//! Retention: keep only what the entry can reach.
//!
//! A compiled binary carries the linked model as its payload, so retention
//! drops every declaration no running body reaches: an unused
//! standard-library helper, an unused user function, or a top-level value
//! nothing reads. It walks from `Program.entry`, collects every declaration a
//! reached body calls or references, then rewrites the model to that set with
//! fresh `DeclarationId`s. Every edge is remapped, so a retained node cannot
//! point at a dropped one, and the structural check linking runs is repeated
//! over the compacted model.

mod collect;
mod retain;

#[cfg(test)]
mod tests;

pub(crate) use retain::retain;
