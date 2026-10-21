//! The compiler engine: loading, name resolution, validation, pruning,
//! lowering, and packaging.
//!
//! A compile walks the stages in order, and this facade is the one place that
//! names them. The build runs the first half through one pipeline: `load_files`
//! walks the entry file's directory tree and the embedded library through the
//! source the caller names; `resolve_names` builds the program model and
//! resolves every name into an edge; `validate_program` derives every type and
//! enforces the body and flow rules. What a build leaves is the checked
//! program and the warnings its unused syntax earned. The pipeline then
//! `prune_unreachable` keeps only what the entry reaches, `lower_bytecode`
//! lowers the result to a compact instruction stream, and `package_binary`
//! serializes that bytecode behind the binary trailer. The model it builds
//! lives in `crate::compiler::model`, and the bytecode it produces lives in
//! `crate::compiler::bytecode`.

mod build;
pub(crate) mod bytecode;
mod completions;
mod diagnostic;
mod load_files;
mod lower_bytecode;
mod lower_expressions;
mod messages;
pub(crate) mod model;
pub(crate) mod native_registry;
mod outline;
mod output_file;
mod package_binary;
mod pipeline;
mod prune_unreachable;
mod reads;
mod references;
mod rename;
mod resolve_names;
mod semantic;
mod sources;
#[cfg(test)]
mod test_support;
mod validate_program;
mod warnings;

pub use build::{Build, BuildFailure, FileDiagnostics, build, check};
pub use completions::{Completion, CompletionKind};
pub(crate) use diagnostic::DIAGNOSTIC_PREFIX;
pub use diagnostic::{Diagnostic, RelatedSpan, Severity};
pub(in crate::compiler) use load_files::collect_kiru_files;
pub(crate) use load_files::{LoadedProgram, STD_TREE};
#[cfg(test)]
pub(crate) use lower_bytecode::declaration_codes;
pub(crate) use lower_bytecode::lower_bytecode;
pub(in crate::compiler) use messages::{
    duplicate_field, duplicate_in_namespace, duplicate_name, expected_instead, invalid_name,
    native_used_as_value, no_workspace_file, private_member, value_called,
};
pub use outline::{Outline, OutlineKind};
pub(crate) use package_binary::{
    attach_trailer, deserialize_bytecode, read_trailer, serialize_bytecode,
};
pub(crate) use pipeline::compile;
pub(crate) use prune_unreachable::prune_unreachable;
pub(crate) use resolve_names::{EntryShape, resolve_names};
pub use semantic::{SemanticRole, SemanticToken, TOKEN_MODIFIERS, TOKEN_TYPES};
pub use sources::{DiskSource, OverlaySource, ProgramSource, VirtualSource};
#[cfg(test)]
pub(crate) use test_support::{checked_files, checked_program};
pub(crate) use validate_program::validate_program;
