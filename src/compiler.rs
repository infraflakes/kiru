//! The compiler engine: loading, name resolution, validation, pruning, and
//! packaging.
//!
//! A compile walks the stages in order, and this facade is the one place that
//! names them. `load_files` reads the entry and every import; `resolve_names`
//! builds the program graph and resolves every name into an edge;
//! `validate_program` derives every kind and enforces the body and flow rules;
//! `prune_unreachable` keeps only what the entry reaches; and `package_binary`
//! serializes the result behind the binary trailer. This module re-exports the
//! model and the stage entry points the rest of the crate drives.

mod compile_program;
mod diagnostics;
mod load_files;
mod package_binary;
mod program_model;
mod prune_unreachable;
mod resolve_names;
mod validate_program;

pub(crate) use compile_program::compile;
pub(crate) use diagnostics::Diagnostic;
pub(crate) use load_files::{LoadedProgram, load_files};
pub(crate) use package_binary::{
    attach_trailer, deserialize_program, read_trailer, serialize_program,
};
pub(crate) use program_model::{
    BUILTIN_NAMESPACE, BindingKind, Case, Declaration, DeclarationId, DeclarationKind, Derived,
    ENTRY_FUNCTION, Expression, Field, File, FileId, Function, Kind, NATIVE_ROWS, NameTable,
    Namespace, NamespaceId, Native, Position, Program, Record, Row, Statement, Value, Visitor,
    VisitorMut, fits, namespace_path, native_row, walk_expression, walk_expression_mut,
    walk_statements, walk_statements_mut,
};
pub(crate) use prune_unreachable::prune_unreachable;
pub(crate) use resolve_names::resolve_names;
pub(crate) use validate_program::validate_program;
