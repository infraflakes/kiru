//! The compiler engine: source loading, linking, checking, retention, and
//! embedding.
//!
//! A compile walks the phases in order. The loader reads the entry file and
//! every file it imports; the linker builds the program model and resolves
//! every name into an edge; the checker derives every kind and enforces the
//! body rules; retention keeps only the declarations the entry reaches; and
//! embedding serializes the result behind the binary trailer. This facade
//! re-exports the model and the entry points the rest of the crate drives.

mod check;
mod diagnostics;
mod embed;
mod link;
mod loader;
mod model;
mod reach;

pub(crate) use check::analyze;
pub(crate) use diagnostics::Diagnostic;
pub(crate) use embed::{append, capture, read, restore};
pub(crate) use link::link;
pub(crate) use loader::{LoadedProgram, load};
pub(crate) use model::{
    Accept, BUILTIN_NAMESPACE, BindingKind, Case, Declaration, DeclarationId, DeclarationKind,
    Derived, ENTRY_FUNCTION, Expression, Field, File, FileId, Function, Kind, NATIVE_ROWS,
    NameTable, Namespace, NamespaceId, Native, Position, Program, Record, Statement, Value,
    Visitor, VisitorMut, fits, namespace_path, native_row, walk_expression, walk_expression_mut,
    walk_statements, walk_statements_mut,
};
pub(crate) use reach::retain;
