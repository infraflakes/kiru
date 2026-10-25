//! The derived rules: resolve the given names and check the linked bodies.
//!
//! `resolve` builds the graph and links every body into edges; `check` reads
//! the graph and settles the derived facts. Neither pass writes the graph after
//! resolution.

pub(crate) mod check;
pub(crate) mod resolve;

pub(crate) use check::validate_program;
pub(crate) use resolve::{Analysis, EntryShape, MISSING_MAIN, program_main, resolve_tree};
