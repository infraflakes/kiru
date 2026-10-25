//! The kinds a declaration can have, and every fact each kind declares.
//!
//! A declaration kind is the form a declaration is written in: a plain `fn`, a
//! `let` module value, a `let` local binding, a `rec`, an `enum`, a function
//! filed under a type by an `impl` block, or a native. Every fact about a kind is declared here, once,
//! as one match over `DeclarationKind` with an arm for every kind and no
//! wildcard: a kind that is not handled fails to compile, so a form can never
//! fall through to a silent default. The loader, the resolver, the checker, and
//! the lowering read these facts instead of matching a declaration's syntax
//! form or testing which of its payload fields are present, so a new form is
//! described in this one place.

use crate::compiler::graph::Group;

/// The kind of declaration a node is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum DeclarationKind {
    /// A plain `fn` declared at namespace level.
    Function,
    /// A `let` declared at namespace level: a module value, evaluated once at
    /// load and reached as a global.
    ModuleValue,
    /// A `let` inside a body: a local binding, or a parameter. It is storage
    /// owned by the function that declares it.
    Local,
    /// A `rec Name { ... }` type declaration.
    Rec,
    /// An `enum Name { ... }` type declaration.
    Enum,
    /// The loader's surface form for a function written inside an `impl`
    /// block. Registration normalizes it to `Function` with the method or
    /// associated binding, so no node in the program graph has this kind. Its
    /// name belongs to the type, so it is never a namespace-level name.
    Impl,
    /// A callable the runtime provides.
    Native,
}

impl DeclarationKind {
    /// Whether the form introduces a name.
    pub(crate) fn has_name(self) -> bool {
        match self {
            DeclarationKind::Function => true,
            DeclarationKind::ModuleValue => true,
            DeclarationKind::Local => true,
            DeclarationKind::Rec => true,
            DeclarationKind::Enum => true,
            DeclarationKind::Impl => true,
            DeclarationKind::Native => true,
        }
    }

    /// Whether the form carries an executable body. A function's value is its
    /// body; a `let` carries a value expression instead, and a local binding
    /// carries neither in its declaration.
    pub(crate) fn has_body(self) -> bool {
        match self {
            DeclarationKind::Function => true,
            DeclarationKind::ModuleValue => false,
            DeclarationKind::Local => false,
            DeclarationKind::Rec => false,
            DeclarationKind::Enum => false,
            DeclarationKind::Impl => true,
            DeclarationKind::Native => false,
        }
    }

    /// Whether the form is a value form: a `let`, whether it is a module value
    /// or a local binding.
    pub(crate) fn has_value(self) -> bool {
        self.module_value() || self.local()
    }

    /// Whether the name is a namespace-level name. An `impl` member's name
    /// belongs to its type and a local binding's name belongs to its function,
    /// so neither joins the namespace name pool.
    pub(crate) fn owns_name(self) -> bool {
        match self {
            DeclarationKind::Function => true,
            DeclarationKind::ModuleValue => true,
            DeclarationKind::Local => false,
            DeclarationKind::Rec => true,
            DeclarationKind::Enum => true,
            DeclarationKind::Impl => false,
            DeclarationKind::Native => true,
        }
    }

    /// Whether the form may be reached through a type: an `impl` member, or a
    /// native the registry files under a receiver or associated head.
    pub(crate) fn is_bound(self) -> bool {
        match self {
            DeclarationKind::Function => false,
            DeclarationKind::ModuleValue => false,
            DeclarationKind::Local => false,
            DeclarationKind::Rec => false,
            DeclarationKind::Enum => false,
            DeclarationKind::Impl => true,
            DeclarationKind::Native => true,
        }
    }

    /// Whether the form may be the program's `main`.
    pub(crate) fn may_be_entry(self) -> bool {
        match self {
            DeclarationKind::Function => true,
            DeclarationKind::ModuleValue => false,
            DeclarationKind::Local => false,
            DeclarationKind::Rec => false,
            DeclarationKind::Enum => false,
            DeclarationKind::Impl => false,
            DeclarationKind::Native => false,
        }
    }

    /// Whether a `use` may name the form. A module value, a local binding, and
    /// an `impl` member are never namespace-level names a `use` may reach.
    pub(crate) fn useable(self) -> bool {
        match self {
            DeclarationKind::Function => true,
            DeclarationKind::ModuleValue => false,
            DeclarationKind::Local => false,
            DeclarationKind::Rec => true,
            DeclarationKind::Enum => true,
            DeclarationKind::Impl => false,
            DeclarationKind::Native => true,
        }
    }

    /// Whether the form runs at load: a top-level `let` is evaluated once
    /// before the entry, and a local binding is storage inside a body.
    pub(crate) fn module_value(self) -> bool {
        match self {
            DeclarationKind::Function => false,
            DeclarationKind::ModuleValue => true,
            DeclarationKind::Local => false,
            DeclarationKind::Rec => false,
            DeclarationKind::Enum => false,
            DeclarationKind::Impl => false,
            DeclarationKind::Native => false,
        }
    }

    /// Whether the form is storage inside a function: a parameter or a local
    /// binding. Its name is bare in the body that owns it.
    pub(crate) fn local(self) -> bool {
        match self {
            DeclarationKind::Function => false,
            DeclarationKind::ModuleValue => false,
            DeclarationKind::Local => true,
            DeclarationKind::Rec => false,
            DeclarationKind::Enum => false,
            DeclarationKind::Impl => false,
            DeclarationKind::Native => false,
        }
    }

    /// Whether a value of the form can be called.
    pub(crate) fn callable(self) -> bool {
        match self {
            DeclarationKind::Function => true,
            DeclarationKind::ModuleValue => false,
            DeclarationKind::Local => false,
            DeclarationKind::Rec => false,
            DeclarationKind::Enum => false,
            DeclarationKind::Impl => true,
            DeclarationKind::Native => true,
        }
    }

    /// Whether the form declares a type. This is derived from the facts above
    /// rather than declared as one more match: only a named form with no body
    /// and no call is a type declaration (`rec` and `enum`).
    pub(crate) fn declares_type(self) -> bool {
        self.owns_name() && !self.has_body() && !self.has_value() && !self.callable()
    }

    /// Whether the form is the runtime's, not the program's. A callable with
    /// no body is a native.
    pub(crate) fn native(self) -> bool {
        self.callable() && !self.has_body()
    }

    /// The type group a type declaration belongs to: a `rec` is `Rec`, an
    /// `enum` is `Enum`. Every other form declares no type.
    pub(crate) fn group(self) -> Option<Group> {
        match self {
            DeclarationKind::Rec => Some(Group::Rec),
            DeclarationKind::Enum => Some(Group::Enum),
            DeclarationKind::Function => None,
            DeclarationKind::ModuleValue => None,
            DeclarationKind::Local => None,
            DeclarationKind::Impl => None,
            DeclarationKind::Native => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_declaration_kind_declares_every_fact() {
        // Every kind is reachable, and each fact is total: the match in every
        // fact has an arm for all seven, and a new kind is a compile error in
        // this file rather than a gap. The assertions name the facts.
        let kinds = [
            DeclarationKind::Function,
            DeclarationKind::ModuleValue,
            DeclarationKind::Local,
            DeclarationKind::Rec,
            DeclarationKind::Enum,
            DeclarationKind::Impl,
            DeclarationKind::Native,
        ];
        assert_eq!(
            kinds.len(),
            7,
            "the model has exactly seven declaration kinds"
        );

        for kind in kinds {
            let _ = kind.has_name();
            let _ = kind.has_body();
            let _ = kind.has_value();
            let _ = kind.owns_name();
            let _ = kind.is_bound();
            let _ = kind.may_be_entry();
            let _ = kind.useable();
            let _ = kind.module_value();
            let _ = kind.local();
            let _ = kind.callable();
            let _ = kind.declares_type();
            let _ = kind.native();
            let _ = kind.group();
        }

        assert!(DeclarationKind::Function.may_be_entry());
        assert!(DeclarationKind::Function.useable());
        assert!(DeclarationKind::Function.callable());
        assert!(DeclarationKind::Function.has_body());
        assert!(!DeclarationKind::Function.module_value());

        assert!(DeclarationKind::ModuleValue.has_value());
        assert!(DeclarationKind::ModuleValue.module_value());
        assert!(!DeclarationKind::ModuleValue.local());
        assert!(!DeclarationKind::ModuleValue.useable());
        assert!(!DeclarationKind::ModuleValue.callable());

        assert!(DeclarationKind::Local.has_value());
        assert!(DeclarationKind::Local.local());
        assert!(!DeclarationKind::Local.module_value());
        assert!(!DeclarationKind::Local.owns_name());
        assert!(!DeclarationKind::Local.useable());
        assert!(!DeclarationKind::Local.callable());

        assert_eq!(DeclarationKind::Rec.group(), Some(Group::Rec));
        assert_eq!(DeclarationKind::Enum.group(), Some(Group::Enum));
        assert!(DeclarationKind::Rec.declares_type());
        assert!(DeclarationKind::Enum.declares_type());
        assert!(DeclarationKind::Rec.useable());
        assert!(DeclarationKind::Enum.useable());

        assert!(DeclarationKind::Impl.is_bound());
        assert!(!DeclarationKind::Impl.owns_name());
        assert!(!DeclarationKind::Impl.useable());
        assert!(!DeclarationKind::Impl.may_be_entry());

        assert!(DeclarationKind::Native.callable());
        assert!(DeclarationKind::Native.native());
        assert!(DeclarationKind::Native.useable());
        assert!(!DeclarationKind::Native.has_body());
    }
}
