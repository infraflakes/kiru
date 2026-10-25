//! The declaration nodes: a name, the type it holds, and the value it holds.
//!
//! A declaration's value is an expression. A named function, an anonymous
//! closure, a method, and an associated function are one `Function` form; the
//! binding says which one it is. A closure is contained by the body that
//! writes it instead of owning a namespace name, and a method or associated
//! function is filed under a type instead. A parameter or local binding has no
//! value form: its value is storage. A native is the one value the runtime
//! provides rather than the program, so it carries a marker instead of an
//! expression.
//!
//! A declared type is one declaration node too: its name, span, file,
//! visibility, and namespace are the declaration's ordinary fields, and its
//! parameters, runtime marker, and fields or variants are its type body. A
//! type reference is an edge to that node, never a second identity.

mod group;

pub(crate) use group::DeclarationKind;

use crate::compiler::native_registry::Native;

use super::expressions::{Expression, Function};
use super::ids::{DeclarationId, FileId, NamespaceId};
use crate::compiler::model::{Type, TypeHead};
use crate::syntax::Span;

/// How a function declaration is bound to the program.
///
/// A named function owns a namespace name and has no capturing environment.
/// An anonymous function owns no name: it is contained by the body that writes
/// the closure literal, and it captures the enclosing locals that body reads.
/// A method is filed under its receiver's type head, and an associated
/// function is filed under the type it belongs to; neither owns a namespace
/// name, and the method's first parameter is its receiver.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FunctionBinding {
    Named,
    Anonymous { container: DeclarationId },
    Method { receiver: TypeHead },
    Associated { owner: TypeHead },
}

impl FunctionBinding {
    /// Whether the binding owns a namespace name: only a named function does.
    #[cfg(debug_assertions)]
    pub(crate) fn owns_name(self) -> bool {
        matches!(self, Self::Named)
    }

    /// Whether the binding may be the program's entry: only a named function
    /// may.
    pub(crate) fn may_be_entry(self) -> bool {
        matches!(self, Self::Named)
    }

    /// Whether the binding is a method or an associated function: a function
    /// filed under a type rather than a namespace name or a containing body.
    pub(crate) fn is_method_or_associated(self) -> bool {
        matches!(self, Self::Method { .. } | Self::Associated { .. })
    }

    /// Whether the binding files the function under a type. This is the
    /// function-binding fact, distinct from `DeclarationKind::is_bound`, which
    /// describes the loader's surface form.
    pub(crate) fn is_bound(self) -> bool {
        self.is_method_or_associated()
    }
}

/// One declaration node: its kind, its name, its declared type, and the value
/// it holds. The kind and the function binding are the one place every form
/// fact is read from; a pass never re-derives it from the payload fields.
#[derive(Debug)]
pub(crate) struct Declaration {
    pub(crate) kind: DeclarationKind,
    /// The binding of a function declaration. Every other kind has none: only
    /// a function has a namespace name, a containing body, or a type to be
    /// bound to.
    pub(crate) binding: Option<FunctionBinding>,
    pub(crate) name: String,
    /// The span of the written name, for a query that asks where a
    /// declaration's name stands. A native has none: its span is empty.
    pub(crate) name_span: Span,
    /// Whether another namespace may reach this declaration. A name reached
    /// across namespaces and a bound function read it; a kind that takes no
    /// visibility never sets it.
    pub(crate) public: bool,
    /// The file that declared it; natives and locals have none.
    pub(crate) file: Option<FileId>,
    pub(crate) namespace: NamespaceId,
    /// The function that owns a parameter or local binding.
    pub(crate) owner: Option<DeclarationId>,
    /// The declared type of the name. A callable's type is a function type. A
    /// variant-pattern binding declares its payload type; the type its arm
    /// instantiates is a derived fact, not this field.
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
    /// The body of a declared type: its parameters, runtime marker, and fields
    /// or variants. Every other kind has none. The name, span, file,
    /// visibility, and namespace of the type are the declaration's own fields.
    pub(crate) type_body: Option<TypeBody>,
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

    /// Whether this node is a closure literal's anonymous function.
    pub(crate) fn is_anonymous_function(&self) -> bool {
        matches!(self.binding, Some(FunctionBinding::Anonymous { .. }))
    }

    /// The body that contains an anonymous function: the function or module
    /// value whose expression wrote the closure literal. No other node has a
    /// container beyond its file; a method and an associated function are
    /// filed under a type instead.
    #[cfg(debug_assertions)]
    pub(crate) fn anonymous_container(&self) -> Option<DeclarationId> {
        match self.binding {
            Some(FunctionBinding::Anonymous { container }) => Some(container),
            Some(
                FunctionBinding::Named
                | FunctionBinding::Method { .. }
                | FunctionBinding::Associated { .. },
            )
            | None => None,
        }
    }

    /// Whether the node owns a namespace name. The binding decides for a
    /// function: a named function owns its name, while an anonymous function
    /// belongs to the body that writes it and a method or associated function
    /// belongs to its type. A node with no function binding falls back to its
    /// kind.
    #[cfg(debug_assertions)]
    pub(crate) fn owns_name(&self) -> bool {
        match self.binding {
            Some(binding) => binding.owns_name(),
            None => self.kind.owns_name(),
        }
    }

    /// Whether the node may be the program's entry. The binding decides for a
    /// function: only a named function may be the entry. A node with no
    /// function binding falls back to its kind.
    pub(crate) fn may_be_entry(&self) -> bool {
        match self.binding {
            Some(binding) => binding.may_be_entry(),
            None => self.kind.may_be_entry(),
        }
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
    pub(crate) fn return_type(&self, declarations: &[Declaration]) -> Option<Type> {
        self.ty.return_type(declarations)
    }

    /// The type parameters a call binds: a function's declared ones, or the
    /// parameters a native's row mentions, in order of first appearance. A
    /// native has no function node, so its row's function type is the
    /// declaration.
    pub(crate) fn type_parameters(&self, declarations: &[Declaration]) -> Vec<String> {
        match self.function() {
            Some(function) => function.type_parameters.clone(),
            None => crate::compiler::model::type_parameters_of(&self.ty, declarations),
        }
    }

    /// The body of a declared type, when this node declares one.
    pub(crate) fn type_body(&self) -> Option<&TypeBody> {
        self.type_body.as_ref()
    }

    /// The body of a declared type, asserting the node is one. A caller that
    /// has already proved the declaration's kind reads its body here.
    #[cfg(debug_assertions)]
    pub(crate) fn expect_type_body(&self) -> &TypeBody {
        debug_assert!(
            matches!(self.kind, DeclarationKind::Rec | DeclarationKind::Enum),
            "a type body belongs to a rec or enum declaration"
        );
        self.type_body
            .as_ref()
            .expect("a type declaration carries its type body")
    }
}

/// The body of a declared type: what the type is made of, beside the identity
/// the declaration node carries. The name, written span, file, visibility, and
/// namespace are the declaration's fields, not the body's.
#[derive(Debug)]
pub(crate) struct TypeBody {
    /// The type parameters in `Name<A, B>`, in order; empty when there are
    /// none.
    pub(crate) parameters: Vec<String>,
    /// Whether the group's representation describes the type, or the runtime
    /// builds and changes its values itself. The runtime marks the one type it
    /// represents directly; every other declaration is described by its group.
    pub(crate) runtime: RuntimeType,
    pub(crate) kind: TypeKind,
}

/// Which side describes a declared type's values.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RuntimeType {
    /// The declaration's group describes the value: a `rec` is built from its
    /// fields and an `enum` from its variants.
    Declared,
    /// The runtime represents the value itself: it is built and changed only
    /// through the type's own operations, and it has no literal form. The
    /// compiler registers the one such declaration, the runtime's list, whose
    /// single type parameter is its element.
    List,
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
    /// The span of the field's written name.
    pub(crate) name_span: Span,
    /// Whether another namespace may read and write the field.
    pub(crate) public: bool,
    pub(crate) ty: Type,
}

/// A written type name in a file: the path it spells and the name the path
/// ends at. The written name resolves to a type declaration; this records where
/// the name stands so a later pass can paint it.
#[derive(Clone, Copy, Debug)]
pub(crate) struct TypeUse {
    pub(crate) file: FileId,
    pub(crate) path: Span,
    pub(crate) name: Span,
}

/// One variant of a declared enum type; `payload` holds its positional types.
#[derive(Debug)]
pub(crate) struct VariantDecl {
    pub(crate) name: String,
    /// The span of the variant's written name.
    pub(crate) name_span: Span,
    /// Whether another namespace may construct and match the variant.
    pub(crate) public: bool,
    pub(crate) payload: Vec<Type>,
}
