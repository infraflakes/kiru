//! The compiler engine: loading, name resolution, validation, pruning,
//! lowering, and packaging.
//!
//! A compile walks the stages in order, and this facade is the one place that
//! names them. The build runs the first half through one pipeline: `load_files`
//! reads the entry file's directory tree through the program source the caller
//! supplies and mounts the embedded library; `resolve_program` builds the
//! program model, resolves every name into an edge, and selects the entry;
//! `validate_program` derives every type and enforces the body and flow rules.
//! What a build leaves is the checked program, the entry it starts from, and
//! the warnings its unused syntax earned. The pipeline then marks what the
//! entry reaches, `prune_unreachable` keeps only that, `lower_bytecode` lowers
//! the result to a compact instruction stream, and `package_binary` serializes
//! that bytecode behind the binary trailer. A single file is analyzed the same
//! first half: `analyze_file` resolves only that file's bodies and checks only
//! them, with no entry and no pruning. The compiler reads no disk; the command
//! line, the language server, and the tests supply the source. The model it
//! builds lives in `crate::compiler::model`, and the bytecode it produces lives
//! in `crate::compiler::bytecode`.

mod build;
pub(crate) mod bytecode;
mod diagnostic;
mod load_files;
mod lower_bytecode;
mod lower_expressions;
mod messages;
pub(crate) mod model;
pub(crate) mod native_registry;
mod output_file;
mod package_binary;
mod pipeline;
mod prune_unreachable;
mod reads;
mod resolve_names;
mod semantic;
mod sources;
#[cfg(test)]
mod test_support;
mod validate_program;
mod warnings;

pub(crate) use build::semantic_tokens;
pub use build::{AnalysisContext, Build, BuildFailure, FileDiagnostics, analyze_file, build};
pub(crate) use diagnostic::DIAGNOSTIC_PREFIX;
pub use diagnostic::{Diagnostic, RelatedSpan, Severity};
pub(crate) use load_files::{LoadedProgram, STD_TREE};
#[cfg(test)]
pub(crate) use lower_bytecode::declaration_codes;
pub(crate) use lower_bytecode::lower_bytecode;
pub(in crate::compiler) use messages::{
    duplicate_field, duplicate_in_namespace, duplicate_name, expected_instead,
    native_used_as_value, private_member, value_called,
};
pub(crate) use package_binary::{
    attach_trailer, deserialize_bytecode, payload_in, serialize_bytecode,
};
pub(crate) use pipeline::compile;
pub(crate) use prune_unreachable::{mark_reachable, prune_unreachable};
pub(crate) use resolve_names::EntryShape;
#[cfg(test)]
pub(crate) use semantic::SemanticRole;
pub(crate) use semantic::{SemanticToken, TOKEN_MODIFIERS, TOKEN_TYPES};
pub use sources::ProgramSource;
#[cfg(test)]
pub(crate) use test_support::{
    OverlayOnDisk, VirtualSource, checked_files, checked_program, checked_program_and_entry,
};
pub(crate) use validate_program::{validate_file, validate_program};
pub(crate) use warnings::unused_declarations;
