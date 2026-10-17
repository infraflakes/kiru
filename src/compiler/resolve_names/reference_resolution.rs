//! Resolving a name into an edge.
//!
//! A bare name reads a local first, then its own namespace and the names a
//! `use` brings in by name; a qualified path starts at a `use`d namespace and
//! walks into it. A call reads the method pool by its first argument's type
//! head first, then falls back to the name pool. Resolution is order-free: any
//! declaration in the program may be named.

use crate::compiler::Diagnostic;
use crate::model::{DeclarationId, NamespaceId, join_path, namespace_at};
use crate::model::{Type, TypeHead};
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
        self.resolve_name(context, path, false, span)
    }

    /// Resolve a direct call `name(arguments)`. The first argument's type head
    /// selects a method from the method pool, exactly as a method call does; a
    /// call that names no such method falls back to the name pool, where a
    /// function with no parameters is preferred, so a no-argument call is not
    /// made ambiguous by a family that shares its name.
    pub(super) fn resolve_call(
        &self,
        context: &Context,
        path: &[String],
        head: Option<TypeHead>,
        span: Span,
    ) -> Result<DeclarationId, Diagnostic> {
        let reported = &self.files[context.file.0].path;
        // A local binding is a value, not a named method.
        if path.len() == 1
            && let Some(found) = context.scopes.find(&path[0])
        {
            return Ok(found);
        }
        if let Some(head) = head {
            let found = self.call_methods(context, path, head);
            if found.len() > 1 {
                return Err(Diagnostic::new(
                    reported,
                    span,
                    format!("`{}` is ambiguous for this value", join_path(path)),
                ));
            }
            if let Some(found) = found.first() {
                return Ok(*found);
            }
        }
        self.resolve_name(context, path, true, span)
    }

    /// Resolve a path through the name pool. With `prefer_no_parameter`, a name
    /// that holds several declarations and exactly one with no first parameter
    /// selects that one: a call with no head names a function with no
    /// parameters.
    fn resolve_name(
        &self,
        context: &Context,
        path: &[String],
        prefer_no_parameter: bool,
        span: Span,
    ) -> Result<DeclarationId, Diagnostic> {
        let reported = &self.files[context.file.0].path;
        let display = join_path(path);
        let found = self.search(context, path);
        let found = if prefer_no_parameter {
            let no_parameter: Vec<DeclarationId> = found
                .iter()
                .copied()
                .filter(|declaration| self.first_parameter_head(*declaration).is_none())
                .collect();
            if no_parameter.len() == 1 {
                no_parameter
            } else {
                found
            }
        } else {
            found
        };
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
    /// `use`d namespace and walks into it. A name reached across a namespace
    /// must be `pub`.
    fn search(&self, context: &Context, path: &[String]) -> Vec<DeclarationId> {
        let (name, prefix) = path
            .split_last()
            .expect("a name path has at least one segment");
        if prefix.is_empty() {
            return self.bare_declarations(context.namespace, name);
        }
        let Some(base) = self.used_namespace(context.namespace, &prefix[0]) else {
            return Vec::new();
        };
        let rest = &prefix[1..];
        let Some(namespace) = self.namespace_at_from(base, rest) else {
            return Vec::new();
        };
        // A declaration reached in its own namespace is visible whatever its
        // visibility; across a namespace it must be `pub`.
        self.namespaces[namespace.0]
            .get(name)
            .iter()
            .copied()
            .filter(|declaration| {
                namespace == context.namespace || self.skeletons[declaration.0].public
            })
            .collect()
    }

    /// Every declaration a `use` brings under `name` for dispatch: a namespace
    /// `use` contributes its own names, a name-`use` only the name it names.
    fn used_declarations(&self, used: &[String], name: &str) -> Vec<DeclarationId> {
        let base = if let Some(base) = namespace_at(&self.namespaces, used) {
            base
        } else if let Some(base) = self.name_use(used, name) {
            base
        } else {
            return Vec::new();
        };
        self.namespaces[base.0]
            .get(name)
            .iter()
            .copied()
            .filter(|declaration| self.skeletons[declaration.0].public)
            .collect()
    }

    /// Every visible declaration of a name: all of the namespace's own, then
    /// every public declaration of each namespace it `use`s. A name usually
    /// holds one declaration; a method family holds one per receiver head.
    pub(super) fn visible_declarations(
        &self,
        namespace: NamespaceId,
        name: &str,
    ) -> Vec<DeclarationId> {
        let mut found: Vec<DeclarationId> = self.namespaces[namespace.0].get(name).to_vec();
        for used in &self.namespaces[namespace.0].uses {
            for declaration in self.used_declarations(used, name) {
                if !found.contains(&declaration) {
                    found.push(declaration);
                }
            }
        }
        found
    }

    /// Every method a call can reach by its first argument's type head: for a
    /// bare name, the current namespace's method pool then each namespace it
    /// `use`s; for a qualified path, the namespace the path reaches. A
    /// constructor registered under the same head is not a method: only a
    /// declaration whose first parameter has the head answers a call.
    fn call_methods(
        &self,
        context: &Context,
        path: &[String],
        head: TypeHead,
    ) -> Vec<DeclarationId> {
        let (name, prefix) = path
            .split_last()
            .expect("a call path has at least one segment");
        if prefix.is_empty() {
            let mut found = Vec::new();
            if let Some(declaration) = self.namespaces[context.namespace.0].method(head, name)
                && self.first_parameter_head(declaration) == Some(head)
            {
                found.push(declaration);
            }
            for used in &self.namespaces[context.namespace.0].uses {
                for declaration in self.used_method(used, name, head) {
                    if !found.contains(&declaration) {
                        found.push(declaration);
                    }
                }
            }
            return found;
        }
        let Some(base) = self.used_namespace(context.namespace, &prefix[0]) else {
            return Vec::new();
        };
        let rest = &prefix[1..];
        let Some(namespace) = self.namespace_at_from(base, rest) else {
            return Vec::new();
        };
        match self.namespaces[namespace.0].method(head, name) {
            Some(declaration)
                if (namespace == context.namespace || self.skeletons[declaration.0].public)
                    && self.first_parameter_head(declaration) == Some(head) =>
            {
                vec![declaration]
            }
            _ => Vec::new(),
        }
    }

    /// A method a `use` brings under `name` for a receiver head.
    fn used_method(&self, used: &[String], name: &str, head: TypeHead) -> Vec<DeclarationId> {
        let base = if let Some(base) = namespace_at(&self.namespaces, used) {
            base
        } else if let Some(base) = self.name_use(used, name) {
            base
        } else {
            return Vec::new();
        };
        self.namespaces[base.0]
            .method(head, name)
            .filter(|declaration| {
                self.skeletons[declaration.0].public
                    && self.first_parameter_head(*declaration) == Some(head)
            })
            .into_iter()
            .collect()
    }

    /// The type head of a callable's first parameter: the receiver a method
    /// call on it must have. `None` when the declaration is not a callable
    /// with a first parameter.
    fn first_parameter_head(&self, declaration: DeclarationId) -> Option<TypeHead> {
        if let Some(head) = self.skeletons[declaration.0].first_parameter_head {
            return Some(head);
        }
        let ty = &self.declarations[declaration.0].as_ref()?.ty;
        match ty {
            Type::Function { parameters, .. } => parameters.first().map(Type::head),
            _ => None,
        }
    }

    /// The function named `name`, visible to `context`, whose first parameter
    /// accepts a receiver with `head`. The receiver's type selects it among
    /// every visible declaration of that name. `None` when none accepts the
    /// receiver; two that both accept it are a conflict.
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
        let mut selected = None;
        for declaration in self.visible_declarations(context.namespace, name) {
            if self.first_parameter_head(declaration) != Some(head) {
                continue;
            }
            if selected.is_some() {
                return Err(Diagnostic::new(
                    &self.files[context.file.0].path,
                    span,
                    format!("`{name}` is ambiguous for this value"),
                ));
            }
            selected = Some(declaration);
        }
        Ok(selected)
    }

    /// The diagnostic for a method name no visible function accepts: point at
    /// the namespace that declares one, when one exists, so the fix is a `use`.
    pub(super) fn method_not_found(
        &self,
        context: &Context,
        name: &str,
        head: Option<TypeHead>,
        span: Span,
    ) -> Diagnostic {
        let reported = &self.files[context.file.0].path;
        if let Some(head) = head
            && let Some(namespace) = self.namespace_declaring_method(name, head)
        {
            let path = crate::model::namespace_path(&self.namespaces, namespace);
            let path = crate::model::join_path(&path);
            return Diagnostic::new(
                reported,
                span,
                format!("`{name}` is declared in `{path}`; add `use {path};` to reach it"),
            );
        }
        Diagnostic::new(reported, span, format!("no method `{name}` for this value"))
    }

    /// A namespace that declares a public function named `name` whose first
    /// parameter accepts a receiver with `head`, for a `use` hint.
    fn namespace_declaring_method(&self, name: &str, head: TypeHead) -> Option<NamespaceId> {
        for (index, namespace) in self.namespaces.iter().enumerate() {
            for declaration in namespace.get(name) {
                if self.skeletons[declaration.0].public
                    && self.first_parameter_head(*declaration) == Some(head)
                {
                    return Some(NamespaceId(index));
                }
            }
        }
        None
    }
}
