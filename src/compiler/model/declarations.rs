//! The declaration nodes: a name, the type it holds, and the value it holds.
//!
//! A declaration's value is an expression. A named function is a function
//! expression, the same node as a `fn(...) { .. }` literal, so a function is
//! not a separate kind of value. A parameter or local binding has no value
//! form: its value is storage. A native is the one value the runtime provides
//! rather than the program, so it carries a marker instead of an expression.

mod group;

pub(crate) use group::DeclarationKind;

use crate::native_registry::Native;

use super::expressions::{Expression, Function};
use super::ids::{DeclarationId, FileId, NamespaceId};
use crate::compiler::model::Type;

/// One declaration node: its kind, its name, the type it holds, and the value
/// it holds. The kind is the one place every form fact is read from; a pass
/// never re-derives it from the payload fields.
#[derive(Debug)]
pub(crate) struct Declaration {
    pub(crate) kind: DeclarationKind,
    pub(crate) name: String,
    /// The file that declared it; natives and locals have none.
    pub(crate) file: Option<FileId>,
    pub(crate) namespace: NamespaceId,
    /// The function that owns a parameter or local binding.
    pub(crate) owner: Option<DeclarationId>,
    /// The declared type of the name. A callable's type is a function type.
    pub(crate) ty: Type,
    /// The value the name holds: a function expression or any other
    /// expression. A parameter or local binding holds no value form here — its
    /// value is storage, supplied by the statement that binds it — so this is
    /// `None`.
    pub(crate) value: Option<Expression>,
    /// The runtime index of a native callable. A native is the one value the
    /// runtime provides rather than the program, so it carries this marker
    /// instead of an expression.
    pub(crate) native: Option<Native>,
    pub(crate) derived: Derived,
}

impl Declaration {
    /// The function value when this node holds one.
    pub(crate) fn function(&self) -> Option<&Function> {
        match &self.value {
            Some(Expression::Function(function)) => Some(function),
            _ => None,
        }
    }

    /// The value expression when this node holds one.
    pub(crate) fn value(&self) -> Option<&Expression> {
        self.value.as_ref()
    }

    /// Whether the runtime provides this callable.
    pub(crate) fn is_native(&self) -> bool {
        self.kind.native()
    }

    /// Whether this node is a callable: a function value or a native.
    pub(crate) fn is_callable(&self) -> bool {
        self.kind.callable()
    }

    /// The declared return type of a callable, or `None` for any other name.
    pub(crate) fn return_type(&self, types: &[TypeDecl]) -> Option<Type> {
        self.ty.return_type(types)
    }

    /// The type parameters a call binds: a function's declared ones, or the
    /// parameters a native's row mentions, in order of first appearance. A
    /// native has no function node, so its row's function type is the
    /// declaration.
    pub(crate) fn type_parameters(&self, types: &[TypeDecl]) -> Vec<String> {
        match self.function() {
            Some(function) => function.type_parameters.clone(),
            None => crate::compiler::model::type_parameters_of(&self.ty, types),
        }
    }
}

/// What a phase has learned about a node.
#[derive(Debug, Default)]
pub(crate) struct Derived {
    /// Whether every path of a function ends in `panic;` or a call to a
    /// function that stops the run. A call to such a function ends its
    /// caller's path exactly like `panic;` does.
    pub(crate) halts: bool,
}

/// A nominal type: a `rec` with fields, or an `enum` with variants.
#[derive(Debug)]
pub(crate) struct TypeDecl {
    pub(crate) name: String,
    /// Whether another namespace may reach the type.
    pub(crate) public: bool,
    /// The namespace that declares the type. A field or variant is reached
    /// from here freely and from another namespace only when it is `pub`.
    pub(crate) namespace: NamespaceId,
    /// The type parameters in `Name<A, B>`, in order; empty when there are
    /// none.
    pub(crate) parameters: Vec<String>,
    pub(crate) kind: TypeKind,
}

/// The body of a declared type.
#[derive(Debug)]
pub(crate) enum TypeKind {
    Rec(Vec<TypeFieldDecl>),
    Enum(Vec<VariantDecl>),
}

/// One field of a declared `rec` type.
#[derive(Debug)]
pub(crate) struct TypeFieldDecl {
    pub(crate) name: String,
    /// Whether another namespace may read and write the field.
    pub(crate) public: bool,
    pub(crate) ty: Type,
}

/// One variant of a declared enum type; `payload` holds its positional types.
#[derive(Debug)]
pub(crate) struct VariantDecl {
    pub(crate) name: String,
    /// Whether another namespace may construct and match the variant.
    pub(crate) public: bool,
    pub(crate) payload: Vec<Type>,
}
