//! Resolving a name into an edge.
//!
//! A bare name reads a local first, then its own namespace and the names a
//! `use` brings in by name; a qualified path starts at a tree name and walks
//! from the root, or at a namespace-`use`'s qualifier. A method call reads the
//! method pool by its receiver's type head; a `Type.name(...)` call reads the
//! associated-function pool by the type's head. A bound function is gated by
//! `pub` alone, never by a `use`. Resolution is order-free: any declaration in
//! the program may be named.

use crate::compiler::Diagnostic;
use crate::compiler::model::TypeHead;
use crate::compiler::model::{DeclarationId, NamespaceId, join_path, reachable};
use crate::syntax::Span;

use super::builder::Builder;
use super::scopes::Context;

impl Builder {
    /// Resolve a bare name to a node: a local binding first, then the current
    /// namespace and the names it `use`s. A name with several visible
    /// declarations is ambiguous.
    pub(super) fn resolve(
        &self,
        context: &Context,
        path: &[String],
        span: Span,
    ) -> Result<DeclarationId, Diagnostic> {
        // A bare name reads a local first.
        if path.len() == 1
            && let Some(found) = context.scopes.find(&path[0])
        {
            return Ok(found);
        }
        self.resolve_name(context, path, span)
    }

    /// Resolve a direct call `name(arguments)`. A bare name is a value, so a
    /// bare call reaches a local or a namespace-level name and nothing else. A
    /// qualified path reaches a namespace-level name, or a method when the
    /// first argument's type head selects it: `std::dynamic::len(xs)`. A
    /// qualified path never reaches a bound function by its bare name: a
    /// function inside `impl T` is reached only through `T`.
    pub(super) fn resolve_call(
        &self,
        context: &Context,
        path: &[String],
        head: Option<TypeHead>,
        span: Span,
    ) -> Result<DeclarationId, Diagnostic> {
        // A local binding is a value, not a named method.
        if path.len() == 1
            && let Some(found) = context.scopes.find(&path[0])
        {
            return Ok(found);
        }
        if path.len() > 1 {
            let prefix = &path[..path.len() - 1];
            if let Some(namespace) = self.namespace_of_path(context.namespace, prefix) {
                let (name, _) = path
                    .split_last()
                    .expect("a call path has at least one segment");
                if let Some(head) = head
                    && let Some(declaration) = self.namespaces[namespace.0].method(head, name)
                    && self.bound_reachable(context.namespace, namespace, declaration)
                {
                    return Ok(declaration);
                }
            }
        }
        self.resolve_name(context, path, span)
    }

    /// Resolve a path through the name pool. A name holds one declaration, so
    /// several visible ones come only from a name-`use` that shadows the
    /// current namespace.
    fn resolve_name(
        &self,
        context: &Context,
        path: &[String],
        span: Span,
    ) -> Result<DeclarationId, Diagnostic> {
        let reported = &self.files[context.file.0].path;
        let display = join_path(path);
        let found = self.search(context, path);
        let [found] = found.as_slice() else {
            let message = if found.is_empty() {
                format!("unknown name `{display}`")
            } else {
                format!("`{display}` is ambiguous for this value")
            };
            return Err(Diagnostic::new(reported, span, message));
        };
        Ok(*found)
    }

    /// Search the name pool for a path. A bare single segment reads the current
    /// namespace and the names it `use`s by name; a qualified path starts at a
    /// tree name and walks from the root, or at a namespace-`use`'s qualifier.
    /// A name reached across a namespace must be `pub`.
    fn search(&self, context: &Context, path: &[String]) -> Vec<DeclarationId> {
        let (name, prefix) = path
            .split_last()
            .expect("a name path has at least one segment");
        if prefix.is_empty() {
            return self.bare_declarations(context.namespace, name);
        }
        let Some(namespace) = self.namespace_of_path(context.namespace, prefix) else {
            return Vec::new();
        };
        // A declaration reached in its own namespace is visible whatever its
        // visibility; across a namespace it must be `pub`.
        self.namespaces[namespace.0]
            .get(name)
            .iter()
            .copied()
            .filter(|declaration| {
                reachable(
                    self.skeletons[declaration.0].public,
                    namespace,
                    context.namespace,
                )
            })
            .collect()
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
        let mut found = Vec::new();
        for (index, namespace) in self.namespaces.iter().enumerate() {
            if let Some(declaration) = namespace.method(head, name)
                && self.bound_reachable(context.namespace, NamespaceId(index), declaration)
            {
                found.push(declaration);
            }
        }
        found
    }

    /// The method named `name`, visible to `context`, whose receiver head is
    /// `head`. `None` when the method pool has none; two are a conflict.
    pub(super) fn select_method(
        &self,
        context: &Context,
        name: &str,
        head: Option<TypeHead>,
        span: Span,
    ) -> Result<Option<DeclarationId>, Diagnostic> {
        let Some(head) = head else {
            return Ok(None);
        };
        let found = self.visible_methods(context, name, head);
        match found.as_slice() {
            [] => Ok(None),
            [declaration] => Ok(Some(*declaration)),
            _ => Err(Diagnostic::new(
                &self.files[context.file.0].path,
                span,
                format!("`{name}` is ambiguous for this value"),
            )),
        }
    }
}
