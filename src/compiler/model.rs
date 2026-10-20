//! The program model: the linked program graph, the type system, the
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
mod program;
mod types;
mod verify_graph;
mod verify_spans;
mod visitors;

pub(crate) use declarations::{
    Declaration, DeclarationKind, Derived, RuntimeType, TypeDecl, TypeFieldDecl, TypeKind,
    VariantDecl,
};
pub(crate) use expressions::{
    Expression, Field, Function, MatchArm, MatchBody, Pattern, Statement, StringPart,
};
pub(crate) use ids::{DeclarationId, FileId, NamespaceId};
/// The name hash a test reads to pin the compiled image's bytes; nothing in
/// the model itself reads it.
#[cfg(test)]
pub(crate) use program::name_hash;
pub(crate) use program::{
    File, Interner, MethodTable, ModelView, NameTable, Namespace, Program, Symbol, TypeTable,
    UseTarget, bound_reachable, first_duplicate, join_path, namespace_at, reachable, rec_field,
};
pub(crate) use types::{
    Arity, Binding, Finite, Group, RuntimeShape, Shape, Type, TypeHead, TypeId, finite, fits,
    has_parameter, list_type, scalar_from_name, substitute, type_name, type_parameters_of, unify,
};
pub(crate) use verify_graph::verify_program;
pub(crate) use visitors::{
    Visitor, VisitorMut, walk_expression, walk_expression_mut, walk_statements,
};

/// The entry function name the runtime calls.
pub(crate) const ENTRY_FUNCTION: &str = "main";
