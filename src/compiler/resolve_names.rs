//! Resolving names: build the program graph from parsed files.
//!
//! This stage registers every declaration into its namespace, resolves every
//! name into an edge to a node, and selects the entry: the entry file's own
//! root `main`. After this stage, no later stage resolves a name again.

mod builder;
mod linking;
mod reference_linking;
mod reference_resolution;
mod register_builtins;
mod register_files;
mod scopes;
mod select_entry;

#[cfg(test)]
mod tests;

pub(crate) use linking::resolve_names;
