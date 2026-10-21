//! Resolving a name into an edge.
//!
//! A bare name reads a local first, then its own namespace and the names a
//! `use` brings in by name; a qualified path starts at a tree name and walks
//! from the root, or at a namespace-`use`'s qualifier. A method call is
//! resolved by the check, which keys the method pool on the receiver's type
//! head; a `Type.name(...)` call reads the associated-function pool by the
//! type's head. A bound function is gated by `pub` alone, never by a `use`.
//! Resolution is order-free: any declaration in the program may be named.

use crate::compiler::Diagnostic;
use crate::compiler::model::TypeHead;
use crate::compiler::model::{DeclarationId, NamespaceId, join_path, reachable};
use crate::syntax::Span;

use super::builder::Builder;
use super::scopes::Context;

/// The callee a call links to: a declaration the name pools name, or the
/// bound-function family a qualified path names, which only the receiver's
/// type head selects — a head the check derives, so the family stays open
/// until the check.
pub(super) enum CalleeLink {
    Declaration(DeclarationId),
    Family(NamespaceId, Vec<String>),
}

impl Builder {
    /// Resolve a bare name to a node: a local binding first, then the current
    /// namespace and the names it `use`s.
    pub(super) fn resolve(
        &self,
        context: &Context,
        path: &[String],
        span: Span,
    ) -> Result<DeclarationId, Diagnostic> {
        // A bare name reads a local first.
        if path.len() == 1
            && let Some(symbol) = self.intern.resolve(&path[0])
            && let Some(found) = context.scopes.find(symbol)
        {
            return Ok(found);
        }
        self.resolve_name(context, path, span)
    }

    /// Resolve a direct call `name(arguments)`. A bare name is a value, so a
    /// bare call reaches a local or a namespace-level name and nothing else; a
    /// qualified path reaches a namespace-level name. A method is selected by
    /// its receiver's type head, and a head is a type fact the check derives,
    /// so a qualified name the pools do not own stays open as the family it
    /// named and the check resolves it. A qualified path never reaches a bound
    /// function by its bare name: a function inside `impl T` is reached only
    /// through `T`.
    pub(super) fn resolve_call(
        &self,
        context: &Context,
        path: &[String],
        span: Span,
    ) -> Result<CalleeLink, Diagnostic> {
        // A local binding is a value, not a named method.
        if path.len() == 1
            && let Some(symbol) = self.intern.resolve(&path[0])
            && let Some(found) = context.scopes.find(symbol)
        {
            return Ok(CalleeLink::Declaration(found));
        }
        if let Some(found) = self.search(context, path) {
            return Ok(CalleeLink::Declaration(found));
        }
        if path.len() > 1 {
            let prefix = &path[..path.len() - 1];
            if let Some(namespace) = self.namespace_of_path(context.namespace, prefix) {
                return Ok(CalleeLink::Family(namespace, path.to_owned()));
            }
        }
        Err(self.unresolved(context, &join_path(path), span))
    }

    /// Resolve a path through the name pool. A name holds one declaration in
    /// its namespace, so a lookup finds one declaration or none.
    fn resolve_name(
        &self,
        context: &Context,
        path: &[String],
        span: Span,
    ) -> Result<DeclarationId, Diagnostic> {
        let Some(found) = self.search(context, path) else {
            return Err(self.unresolved(context, &join_path(path), span));
        };
        Ok(found)
    }

    /// The diagnostic a reference the search cannot name: one message with
    /// the written path, at the reference's span.
    pub(super) fn unresolved(&self, context: &Context, display: &str, span: Span) -> Diagnostic {
        let reported = &self.files[context.file.0].path;
        Diagnostic::new(reported, span, format!("unknown name `{display}`"))
    }

    /// Search the name pool for a path. A bare single segment reads the current
    /// namespace and the names it `use`s by name; a qualified path starts at a
    /// tree name and walks from the root, or at a namespace-`use`'s qualifier.
    /// A name reached across a namespace must be `pub`.
    fn search(&self, context: &Context, path: &[String]) -> Option<DeclarationId> {
        let (name, prefix) = path
            .split_last()
            .expect("a name path has at least one segment");
        if prefix.is_empty() {
            return self.bare_declaration(context.namespace, name);
        }
        let namespace = self.namespace_of_path(context.namespace, prefix)?;
        // A declaration reached in its own namespace is visible whatever its
        // visibility; across a namespace it must be `pub`.
        let declaration = self.namespaces[namespace.0].get(&self.intern, name)?;
        reachable(
            self.skeletons[declaration.0].public,
            namespace,
            context.namespace,
        )
        .then_some(declaration)
    }

    /// Every method for a receiver head named `name` that `context` may reach.
    /// A bound function is gated by `pub` alone, so the whole program is
    /// searched and the caller's `use`s are irrelevant.
    pub(super) fn visible_methods(
        &self,
        context: &Context,
        name: &str,
        head: TypeHead,
    ) -> Vec<DeclarationId> {
        let Some(symbol) = self.intern.resolve(name) else {
            return Vec::new();
        };
        let mut found = Vec::new();
        for (index, namespace) in self.namespaces.iter().enumerate() {
            if let Some(declaration) = namespace.methods.get(head, symbol)
                && self.bound_reachable(context.namespace, NamespaceId(index), declaration)
            {
                found.push(declaration);
            }
        }
        found
    }
}
