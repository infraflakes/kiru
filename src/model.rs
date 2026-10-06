//! The program model: the linked program graph, the kind system, the
//! structural visitors, and the structural verifier.
//!
//! A declaration is a node with identity, a reference is an edge to a node,
//! and results a stage derives are properties of nodes. No stage keeps a table
//! keyed by position; the resolution stage resolves every name, and no later
//! stage repeats that work. The model is the shared intermediate the compiler
//! builds and the runtime reads.

mod declarations;
mod expressions;
mod ids;
mod kinds;
mod program;
mod values;
mod verify;
mod visitors;

pub(crate) use declarations::{BindingKind, Declaration, DeclarationKind, Derived, Function};
pub(crate) use expressions::{Case, Expression, Field, Statement};
pub(crate) use ids::{DeclarationId, FileId, NamespaceId};
pub(crate) use kinds::{Kind, Position, Usage, fits};
pub(crate) use program::{
    File, NameTable, Namespace, Origin, Program, Registry, join_path, namespace_at, namespace_path,
};
pub(crate) use values::{Record, Value};
pub(crate) use verify::verify_program;
pub(crate) use visitors::{
    Visitor, VisitorMut, walk_expression, walk_expression_mut, walk_statements, walk_statements_mut,
};

/// The entry function name the runtime calls.
pub(crate) const ENTRY_FUNCTION: &str = "main";
