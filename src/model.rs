//! The program model: the linked program graph, the type system, the
//! structural visitors, and the structural verifier.
//!
//! A declaration is a node with identity, a reference is an edge to a node,
//! and results a stage derives are properties of nodes. No stage keeps a table
//! keyed by position; the resolution stage resolves every name, and no later
//! stage repeats that work. The model is the shared intermediate the compiler
//! builds and the runtime reads.

mod body;
mod declarations;
mod ids;
mod program;
mod structure;
mod types;
mod visitors;

pub(crate) use body::{
    Expression, Field, Function, MatchArm, MatchBody, Pattern, Statement, StringPart,
};
pub(crate) use declarations::{
    Declaration, Derived, TypeDecl, TypeFieldDecl, TypeKind, VariantDecl,
};
pub(crate) use ids::{DeclarationId, FileId, NamespaceId};
pub(crate) use program::{
    File, MethodTable, NameTable, Namespace, Origin, Program, TypeTable, first_duplicate,
    join_path, namespace_at, namespace_path, rec_field, rec_field_decl,
};
pub(crate) use structure::verify_program;
pub(crate) use types::{
    BuiltinType, Type, TypeHead, TypeId, fits, has_parameter, substitute, substitute_optional,
    type_parameters_of, unify,
};
pub(crate) use visitors::{
    Visitor, VisitorMut, walk_expression, walk_expression_mut, walk_statements,
};

/// The entry function name the runtime calls.
pub(crate) const ENTRY_FUNCTION: &str = "main";
