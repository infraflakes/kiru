//! Linking: build the program model from parsed files.
//!
//! Linking registers every declaration into its namespace, resolves every
//! name into an edge to a node, and selects the entry: the entry file's own
//! root `main`. After linking, no phase resolves a name again.

mod bodies;
mod integrity;
mod registration;

#[cfg(test)]
mod tests;

pub(crate) use bodies::link;
pub(crate) use integrity::verify;
