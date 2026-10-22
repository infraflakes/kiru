//! Resolving names: build the program graph from parsed files.
//!
//! This stage registers every declaration into its namespace, resolves every
//! name into an edge to a node, and selects the entry: the entry file's own
//! root `main`. After this stage, no later stage resolves a name again.

mod binding_linking;
mod body_linking;
mod builder;
mod captures;
mod main;
mod name_resolution;
mod register_builtins;
mod register_declarations;
mod scopes;
mod use_checking;

#[cfg(test)]
mod tests;

pub(crate) use body_linking::{Analysis, resolve_tree};
pub(crate) use main::{EntryShape, program_main};
