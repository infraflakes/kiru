//! The scope stack and the context a body is linked in.

use std::collections::HashMap;

use crate::model::{DeclarationId, FileId, NamespaceId};

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
    stack: Vec<HashMap<String, DeclarationId>>,
}

impl Scopes {
    pub(super) fn push(&mut self) {
        self.stack.push(HashMap::new());
    }

    pub(super) fn pop(&mut self) {
        self.stack.pop();
    }

    pub(super) fn declare(&mut self, name: &str, declaration: DeclarationId) {
        self.stack
            .last_mut()
            .expect("a scope is open while linking a body")
            .insert(name.to_owned(), declaration);
    }

    pub(super) fn find(&self, name: &str) -> Option<DeclarationId> {
        self.stack
            .iter()
            .rev()
            .find_map(|scope| scope.get(name).copied())
    }
}
