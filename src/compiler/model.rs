//! The program model: the linked program graph, the type system, the derived
//! facts, the structural visitors, and the structural verifier.
//!
//! A declaration is a node with identity, and a reference is an edge to a
//! node. Resolution owns the names and edges and writes the graph; no later
//! stage writes it. The facts a stage derives live in the separate `Facts`
//! store, keyed by the node or the expression site they describe, and are
//! disposable: the rules remain the source of truth. The model is the shared
//! intermediate the compiler builds and the runtime reads.

mod declarations;
mod expressions;
mod facts;
mod ids;
mod program;
mod reads;
mod types;
mod verify_graph;
#[cfg(debug_assertions)]
mod verify_relations;
mod verify_spans;
mod visitors;

pub(crate) use declarations::{
    Declaration, DeclarationKind, FunctionBinding, RuntimeType, TypeBody, TypeFieldDecl, TypeKind,
    TypeUse, VariantDecl,
};
pub(crate) use expressions::{
    Expression, Field, Function, MatchArm, MatchBody, Pattern, Statement, StringPart,
};
pub(crate) use facts::{Facts, Site};
pub(crate) use ids::{DeclarationId, FileId, NamespaceId};
/// The name hash a test reads to pin the compiled image's bytes; nothing in
/// the model itself reads it.
#[cfg(test)]
pub(crate) use program::name_hash;
pub(crate) use program::{
    Delivery, File, FileOrigin, Interner, MethodTable, ModelView, NameTable, Namespace, Program,
    Symbol, TypeTable, UseTarget, bound_reachable, first_duplicate, join_path, namespace_at,
    namespace_path, reachable, rec_field,
};
pub(crate) use reads::collect_reads;
pub(crate) use types::{
    Arity, Binding, Finite, Group, RuntimeShape, Shape, Type, TypeHead, finite, fits,
    has_parameter, scalar_from_name, substitute, type_name, type_parameters_of, unify,
};
pub(crate) use verify_graph::verify_program;
#[cfg(debug_assertions)]
pub(crate) use verify_relations::{RelationBoundary, assert_known_relations, verify_reachability};
pub(crate) use visitors::{
    Visitor, VisitorMut, walk_expression, walk_expression_mut, walk_statements,
};

/// The entry function name the runtime calls.
pub(crate) const ENTRY_FUNCTION: &str = "main";
