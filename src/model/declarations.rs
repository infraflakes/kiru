//! The declaration nodes: a function, a module value, a binding, or a native.

use crate::native_registry::Native;
use crate::syntax::Span;

use super::expressions::{Expression, Statement};
use super::ids::{DeclarationId, FileId, NamespaceId};
use super::kinds::Kind;

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
            DeclarationKind::Text(_)
            | DeclarationKind::Record(_)
            | DeclarationKind::List(_)
            | DeclarationKind::Binding(_)
            | DeclarationKind::Native(_) => None,
        }
    }

    /// The function body when this node is a function.
    pub(crate) fn function_mut(&mut self) -> Option<&mut Function> {
        match &mut self.kind {
            DeclarationKind::Function(function) => Some(function),
            DeclarationKind::Text(_)
            | DeclarationKind::Record(_)
            | DeclarationKind::List(_)
            | DeclarationKind::Binding(_)
            | DeclarationKind::Native(_) => None,
        }
    }

    /// The initializer expression when this node is a text, record, or list
    /// value.
    pub(crate) fn initializer(&self) -> Option<&Expression> {
        match &self.kind {
            DeclarationKind::Text(expression)
            | DeclarationKind::Record(expression)
            | DeclarationKind::List(expression) => Some(expression),
            DeclarationKind::Function(_)
            | DeclarationKind::Binding(_)
            | DeclarationKind::Native(_) => None,
        }
    }

    /// The initializer expression when this node is a text, record, or list
    /// value.
    pub(crate) fn initializer_mut(&mut self) -> Option<&mut Expression> {
        match &mut self.kind {
            DeclarationKind::Text(expression)
            | DeclarationKind::Record(expression)
            | DeclarationKind::List(expression) => Some(expression),
            DeclarationKind::Function(_)
            | DeclarationKind::Binding(_)
            | DeclarationKind::Native(_) => None,
        }
    }
}

/// What a phase has learned about a node.
#[derive(Debug, Default)]
pub(crate) struct Derived {
    /// The value's kind, a binding's kind, or a function's return kind. A
    /// function seeds this from its declaration at link time, so a call reads
    /// the kind the declaration fixed without looking at the body.
    pub(crate) kind: Option<Kind>,
    /// Whether every path of a function ends in `panic;` or a call to a
    /// function that stops the run. A call to such a function ends its
    /// caller's path exactly like `panic;` does.
    pub(crate) halts: bool,
}

/// What a declaration node is.
#[derive(Debug)]
pub(crate) enum DeclarationKind {
    Function(Function),
    /// A module-level `txt`; the startup pass evaluates it once and it holds
    /// text.
    Text(Expression),
    /// A module-level `rec`; the startup pass evaluates it once. The
    /// expression is a record literal, a record variable, or a call returning
    /// a record.
    Record(Expression),
    /// A module-level `list`; the startup pass evaluates it once. The
    /// expression is a list literal, a list variable, or a call returning a
    /// list.
    List(Expression),
    /// A parameter or local binding.
    Binding(BindingKind),
    Native(Native),
}

/// The role of a binding node. `mutable` marks a `mut` binding: it may be
/// reassigned and, for a record, have its fields assigned. Module values are
/// never bindings and are always immutable.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BindingKind {
    /// A function parameter.
    Parameter { mutable: bool },
    /// A `txt` binding; it holds text.
    Text { mutable: bool },
    /// A `rec` binding.
    Record { mutable: bool },
    /// A `list` binding.
    List { mutable: bool },
}

impl BindingKind {
    /// Whether this binding may be reassigned or have its fields assigned.
    pub(crate) fn mutable(self) -> bool {
        match self {
            BindingKind::Parameter { mutable }
            | BindingKind::Text { mutable }
            | BindingKind::Record { mutable }
            | BindingKind::List { mutable } => mutable,
        }
    }
}

/// A function body; its parameters are separate nodes. `return_kind` is the
/// kind the declaration fixes: `Nothing` when no arrow is written.
#[derive(Debug)]
pub(crate) struct Function {
    pub(crate) body: Vec<Statement>,
    pub(crate) return_kind: Kind,
}
