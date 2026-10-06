//! The compiler engine: loading, name resolution, validation, pruning,
//! lowering, and packaging.
//!
//! A compile walks the stages in order, and this facade is the one place that
//! names them. `load_files` reads the entry and every import; `resolve_names`
//! builds the program model and resolves every name into an edge;
//! `validate_program` derives every kind and enforces the body and flow rules;
//! `prune_unreachable` keeps only what the entry reaches; `lower_bytecode`
//! lowers the result to a compact instruction stream; and `package_binary`
//! serializes that bytecode behind the binary trailer. The model it builds
//! lives in `crate::model`, and the bytecode it produces lives in
//! `crate::bytecode`.

/// The name the compiler reports under. The runtime reports under the
/// program's own name instead.
pub(crate) const DIAGNOSTIC_PREFIX: &str = "kc";

mod diagnostics;
mod load_files;
mod lower_bytecode;
mod package_binary;
mod pipeline;
mod prune_unreachable;
mod resolve_names;
#[cfg(test)]
mod test_support;
mod validate_program;

pub(in crate::compiler) use diagnostics::{
    Diagnostic, duplicate_in_namespace, duplicate_name, expected_instead, function_used_as_value,
    value_called,
};
pub(crate) use load_files::{LoadedProgram, load_files};
pub(crate) use lower_bytecode::lower_bytecode;
pub(crate) use package_binary::{
    attach_trailer, deserialize_bytecode, read_trailer, serialize_bytecode,
};
pub(crate) use pipeline::compile;
pub(crate) use prune_unreachable::prune_unreachable;
pub(crate) use resolve_names::resolve_names;
#[cfg(test)]
pub(crate) use test_support::{checked_files, checked_program};
pub(crate) use validate_program::validate_program;
