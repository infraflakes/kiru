//! The program graph: files, namespaces, declarations, bindings, and resolved
//! expressions.
//!
//! A declaration is a node with identity, a reference is an edge to a node,
//! and results a stage derives are properties of nodes. No stage keeps a table
//! keyed by position; the resolution stage resolves every name, and no later
//! stage repeats that work.

mod callable_registry;
mod lock;
mod nodes;
mod traverse_program;
mod value_kinds;
mod verify_program;

pub(crate) use callable_registry::{
    BUILTIN_NAMESPACE, ENTRY_FUNCTION, NATIVE_ROWS, Native, NativeRow, native_arity,
};
pub(crate) use lock::{MutexExt, RwLockExt};
pub(crate) use nodes::{
    BindingKind, Case, Declaration, DeclarationId, DeclarationKind, Derived, Expression, Field,
    File, FileId, Function, NameTable, Namespace, NamespaceId, Origin, Program, Record, Registry,
    Statement, Value, join_path, namespace_at, namespace_path,
};
pub(crate) use traverse_program::{
    Visitor, VisitorMut, walk_expression, walk_expression_mut, walk_statements, walk_statements_mut,
};
pub(crate) use value_kinds::{Kind, Position, Usage, fits};
pub(crate) use verify_program::{validate_program_structure, verify_program};
