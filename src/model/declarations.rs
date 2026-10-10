//! The declaration nodes: a function, a module value, a binding, or a native.

use crate::native_registry::Native;
use crate::syntax::Span;

use super::expressions::{Expression, Statement};
use super::ids::{DeclarationId, FileId, NamespaceId};
use crate::types::Type;

/// One declaration or binding node.
#[derive(Debug)]
pub(crate) struct Declaration {
    pub(crate) name: String,
    pub(crate) name_span: Span,
    /// The file that declared it; natives have none.
    pub(crate) file: Option<FileId>,
    pub(crate) namespace: NamespaceId,
    /// Registration order, for the top-down reference rule.
    pub(crate) order: usize,
    /// The function that owns a parameter or local binding.
    pub(crate) owner: Option<DeclarationId>,
    /// The parameter nodes of a callable, in order. A native and a user
    /// function both carry them, so the checker has one call path.
    pub(crate) parameters: Vec<DeclarationId>,
    pub(crate) kind: DeclarationKind,
    pub(crate) derived: Derived,
}

impl Declaration {
    /// The function body when this node is a function.
    pub(crate) fn function(&self) -> Option<&Function> {
        match &self.kind {
            DeclarationKind::Function(function) => Some(function),
            DeclarationKind::Value { .. }
            | DeclarationKind::Binding(_)
            | DeclarationKind::Native(_) => None,
        }
    }

    /// The function body when this node is a function.
    pub(crate) fn function_mut(&mut self) -> Option<&mut Function> {
        match &mut self.kind {
            DeclarationKind::Function(function) => Some(function),
            DeclarationKind::Value { .. }
            | DeclarationKind::Binding(_)
            | DeclarationKind::Native(_) => None,
        }
    }

    /// The initializer expression when this node is a module value.
    pub(crate) fn initializer(&self) -> Option<&Expression> {
        match &self.kind {
            DeclarationKind::Value { expression, .. } => Some(expression),
            DeclarationKind::Function(_)
            | DeclarationKind::Binding(_)
            | DeclarationKind::Native(_) => None,
        }
    }

    /// The initializer expression when this node is a module value.
    pub(crate) fn initializer_mut(&mut self) -> Option<&mut Expression> {
        match &mut self.kind {
            DeclarationKind::Value { expression, .. } => Some(expression),
            DeclarationKind::Function(_)
            | DeclarationKind::Binding(_)
            | DeclarationKind::Native(_) => None,
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

/// What a declaration node is.
#[derive(Debug)]
pub(crate) enum DeclarationKind {
    Function(Function),
    /// A module-level `let`; the startup pass evaluates it once and it holds a
    /// value of its declared type.
    Value {
        ty: Type,
        expression: Expression,
    },
    /// A parameter or local binding.
    Binding(Binding),
    Native(Native),
}

/// A parameter or local binding: the data type it holds and whether it may be
/// reassigned.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Binding {
    pub(crate) ty: Type,
    pub(crate) mutable: bool,
}

/// A function body; its parameters are separate nodes. `return_type` is the
/// type the declaration fixes: `Void` when the function returns no value.
#[derive(Debug)]
pub(crate) struct Function {
    pub(crate) body: Vec<Statement>,
    pub(crate) return_type: Type,
}
