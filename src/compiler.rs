//! The compiler engine: one compile process and one analysis process over one
//! model.
//!
//! `compile` takes a project's tree to an executable: load, resolve, check,
//! find the root `main`, prune, lower, package. `analyze` takes a tree to the
//! facts an editor shows, grouped by file. Both read the same model and the
//! same instruction set; neither reads a program source from disk, since the
//! source is supplied by the command line, the server, or the tests.
//!
//! Every process and every shared fact is an entry file beside its directory:
//! `compile` with its process stages, `analyze` with the editor process, `graph`
//! with the given graph, `derive` with the rules, `views` with the compile and
//! token views, `types` with the type algebra, `verify` with the relational
//! invariants, `operator_facts` with the semantics of an operator, `bytecode`
//! with the instruction set, `diagnostic` with the facts and their rendering,
//! `warnings` with the unused facts, `native_registry` with the native table,
//! and `test_support` with the fixtures.

mod analyze;
pub(crate) mod bytecode;
pub(crate) mod compile;
pub(crate) mod derive;
mod diagnostic;
pub(crate) mod graph;
pub(crate) mod native_registry;
pub(crate) mod operator_facts;
#[cfg(test)]
mod test_support;
pub(crate) mod types;
#[cfg(debug_assertions)]
pub(crate) mod verify;
pub(crate) mod views;
mod warnings;

pub(crate) use analyze::diagnostics as analyze_tree;
pub(crate) use analyze::semantic_tokens;
pub(crate) use compile::build::build;
pub use compile::build::{Build, BuildFailure};
pub(crate) use compile::compile;
pub use compile::load::ProgramSource;
pub(crate) use compile::load::{LoadedProgram, STD_TREE};
#[cfg(test)]
pub(crate) use compile::lower::declaration_codes;
pub(crate) use compile::lower::lower_bytecode;
pub(crate) use compile::package::{
    attach_trailer, deserialize_bytecode, payload_in, serialize_bytecode,
};
#[cfg(test)]
pub(crate) use derive::program_main;
#[cfg(test)]
pub(crate) use derive::validate_program;
pub(crate) use derive::{EntryShape, MISSING_MAIN};
pub(crate) use diagnostic::DIAGNOSTIC_PREFIX;
pub use diagnostic::{Diagnostic, RelatedSpan, Severity};
pub(crate) use diagnostic::{Report, ReportRule};
pub(in crate::compiler) use diagnostic::{
    duplicate_field, duplicate_in_namespace, duplicate_name, expected_instead,
    native_used_as_value, private_member, value_called,
};
#[cfg(test)]
pub(crate) use test_support::{
    VirtualSource, checked_files, checked_program, checked_program_and_entry,
};
#[cfg(debug_assertions)]
pub(crate) use verify::{RelationBoundary, assert_relations, verify_reachability};
#[cfg(test)]
pub(crate) use views::tokens::SemanticRole;
pub(crate) use views::tokens::{SemanticToken, TOKEN_MODIFIERS, TOKEN_TYPES};
pub(crate) use views::{mark_reachable, verify_reachable};
