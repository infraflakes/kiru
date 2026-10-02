//! The program model: files, namespaces, declarations, bindings, and linked
//! expressions.
//!
//! A declaration is a node with identity, a reference is an edge to a node,
//! and results a phase derives are properties of nodes. No phase keeps a
//! table keyed by position; linking resolves every name, and no later phase
//! repeats that work.

mod kinds;
mod nodes;
mod registry;
mod visit;

pub(crate) use kinds::{Kind, Position, fits};
pub(crate) use nodes::{
    BindingKind, Case, Declaration, DeclarationId, DeclarationKind, Derived, Expression, Field,
    File, FileId, Function, NameTable, Namespace, NamespaceId, Program, Record, Statement, Value,
    namespace_path,
};
pub(crate) use registry::{
    Accept, BUILTIN_NAMESPACE, ENTRY_FUNCTION, NATIVE_ROWS, Native, native_row,
};
pub(crate) use visit::{
    Visitor, VisitorMut, walk_expression, walk_expression_mut, walk_statements, walk_statements_mut,
};
