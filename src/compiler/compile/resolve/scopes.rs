//! The scope stack and the context a body is linked in.
//!
//! A scope keys its bindings by the symbol the interner gave the name, so a
//! body's lookups hash nothing.

use std::collections::HashMap;

use crate::compiler::model::{DeclarationId, FileId, NamespaceId, Symbol};

/// The context a body is linked in: the open scopes, the namespace and
/// declaration being linked.
pub(super) struct Context<'a> {
    pub(super) scopes: &'a mut Scopes,
    pub(super) namespace: NamespaceId,
    pub(super) file: FileId,
    pub(super) current: DeclarationId,
    /// The type parameters of the enclosing function, in scope for its
    /// signature and body annotations.
    pub(super) type_parameters: &'a [String],
}

/// The open scopes of one function body.
#[derive(Default)]
pub(super) struct Scopes {
    stack: Vec<HashMap<Symbol, DeclarationId>>,
}

impl Scopes {
    pub(super) fn push(&mut self) {
        self.stack.push(HashMap::new());
    }

    pub(super) fn pop(&mut self) {
        self.stack.pop();
    }

    /// Bind a local's symbol in the innermost scope. The caller interned the
    /// name when it created the local's node, and binds the symbol here.
    pub(super) fn declare(&mut self, symbol: Symbol, declaration: DeclarationId) {
        self.stack
            .last_mut()
            .expect("a scope is open while linking a body")
            .insert(symbol, declaration);
    }

    /// The innermost declaration bound under the symbol, if any.
    pub(super) fn find(&self, symbol: Symbol) -> Option<DeclarationId> {
        self.stack
            .iter()
            .rev()
            .find_map(|scope| scope.get(&symbol).copied())
    }
}
