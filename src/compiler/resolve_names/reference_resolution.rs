//! Resolving a name into an edge.
//!
//! A bare name reads a local first, then the current namespace and the
//! namespaces it `use`s, through the name pool; several visible declarations
//! make it ambiguous. A call reads the method pool by its first argument's
//! type head first, then falls back to the name pool. A rooted or qualified
//! path names a namespace directly. A name cannot reference itself and a
//! forward reference is an error, so a name can only reference declarations
//! made before it.

use crate::compiler::Diagnostic;
use crate::model::{DeclarationId, NamespaceId, join_path, namespace_at};
use crate::model::{Type, TypeHead};
use crate::syntax::Span;

use super::builder::Builder;
use super::scopes::Context;

impl Builder {
    /// Resolve a bare name to a node: a local binding first, then the current
    /// namespace and the namespaces it `use`s, through the name pool. A rooted
    /// path starts at the root namespace. A name with several visible
    /// declarations is ambiguous; a name cannot reference itself and a forward
    /// reference is an error.
    pub(super) fn resolve(
        &self,
        context: &Context,
        path: &[String],
        root: bool,
        span: Span,
    ) -> Result<DeclarationId, Diagnostic> {
        // A bare name reads a local first.
        if !root
            && path.len() == 1
            && let Some(found) = context.scopes.find(&path[0])
        {
            return Ok(found);
        }
        self.resolve_name(context, path, root, false, span)
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
        root: bool,
        head: Option<TypeHead>,
        span: Span,
    ) -> Result<DeclarationId, Diagnostic> {
        let reported = &self.files[context.file.0].path;
        // A local binding is a value, not a named method.
        if !root
            && path.len() == 1
            && let Some(found) = context.scopes.find(&path[0])
        {
            return Ok(found);
        }
        if let Some(head) = head {
            let found = self.call_methods(context, path, root, head);
            if found.len() > 1 {
                return Err(Diagnostic::new(
                    reported,
                    span,
                    format!("`{}` is ambiguous for this value", join_path(path, root)),
                ));
            }
            if let Some(found) = found.first() {
                let display = join_path(path, root);
                self.check_earlier(context, *found, &display, span)?;
                return Ok(*found);
            }
        }
        self.resolve_name(context, path, root, true, span)
    }

    /// Resolve a path through the name pool and apply the top-down rule. With
    /// `prefer_no_parameter`, a name that holds several declarations and
    /// exactly one with no first parameter selects that one: a call with no
    /// head names a function with no parameters.
    fn resolve_name(
        &self,
        context: &Context,
        path: &[String],
        root: bool,
        prefer_no_parameter: bool,
        span: Span,
    ) -> Result<DeclarationId, Diagnostic> {
        let reported = &self.files[context.file.0].path;
        let display = join_path(path, root);
        let found = self.search(context, path, root);
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
        self.check_earlier(context, *found, &display, span)?;
        Ok(*found)
    }

    /// Enforce the top-down rule on a resolved name: it cannot be the name being
    /// declared, and it must be declared before the point that names it. One
    /// rule serves a bare reference and a method dispatch.
    pub(super) fn check_earlier(
        &self,
        context: &Context,
        found: DeclarationId,
        name: &str,
        span: Span,
    ) -> Result<(), Diagnostic> {
        let reported = &self.files[context.file.0].path;
        if found == context.current {
            return Err(Diagnostic::new(
                reported,
                span,
                "a name cannot reference itself",
            ));
        }
        if self.skeletons[found.0].order > context.order {
            return Err(Diagnostic::new(
                reported,
                span,
                format!("`{name}` is declared after this point"),
            ));
        }
        Ok(())
    }

    /// Search the name pool for a path. A bare single segment reads the
    /// namespace itself and the namespaces it `use`s; anything else names a
    /// namespace directly. A name reached across a namespace must be `pub`.
    fn search(&self, context: &Context, path: &[String], root: bool) -> Vec<DeclarationId> {
        let (name, namespace_segments) = path
            .split_last()
            .expect("a name path has at least one segment");
        if !root && namespace_segments.is_empty() {
            return self.visible_declarations(context.namespace, name);
        }
        let namespace = if namespace_segments.is_empty() {
            NamespaceId(0)
        } else {
            match self.namespace_at(namespace_segments) {
                Some(namespace) => namespace,
                None => return Vec::new(),
            }
        };
        self.public_lookup(namespace, name)
    }

    /// Every declaration brought in by one `use` path.
    fn used_declaration(&self, used: &[String], name: &str) -> Vec<DeclarationId> {
        self.used_lookup(
            used,
            name,
            |namespace, name| self.namespaces[namespace.0].get(name).to_vec(),
            |declaration| self.skeletons[declaration.0].public,
        )
    }

    /// Every visible declaration of a name: all of the namespace's own, then
    /// every public declaration of each namespace it uses. A name usually
    /// holds one declaration; a method family holds one per receiver head.
    fn visible_declarations(&self, namespace: NamespaceId, name: &str) -> Vec<DeclarationId> {
        let mut found: Vec<DeclarationId> = self.namespaces[namespace.0].get(name).to_vec();
        for used in &self.namespaces[namespace.0].uses {
            for declaration in self.used_declaration(used, name) {
                if !found.contains(&declaration) {
                    found.push(declaration);
                }
            }
        }
        found
    }

    /// Every method a call can reach by its first argument's type head: for a
    /// bare name, the current namespace's method pool then each namespace it
    /// `use`s; for a path, the named namespace.
    fn call_methods(
        &self,
        context: &Context,
        path: &[String],
        root: bool,
        head: TypeHead,
    ) -> Vec<DeclarationId> {
        let (name, namespace_segments) = path
            .split_last()
            .expect("a call path has at least one segment");
        if !root && namespace_segments.is_empty() {
            let mut found = Vec::new();
            if let Some(declaration) = self.namespaces[context.namespace.0].method(head, name) {
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
        let namespace = if namespace_segments.is_empty() {
            NamespaceId(0)
        } else {
            match self.namespace_at(namespace_segments) {
                Some(namespace) => namespace,
                None => return Vec::new(),
            }
        };
        match self.namespaces[namespace.0].method(head, name) {
            Some(declaration) if self.skeletons[declaration.0].public => vec![declaration],
            _ => Vec::new(),
        }
    }

    /// A method brought in by one `use` path.
    fn used_method(&self, used: &[String], name: &str, head: TypeHead) -> Vec<DeclarationId> {
        self.used_lookup(
            used,
            name,
            |namespace, name| {
                self.namespaces[namespace.0]
                    .method(head, name)
                    .into_iter()
                    .collect()
            },
            |declaration| self.skeletons[declaration.0].public,
        )
    }

    /// The type head of a callable's first parameter: the receiver a method
    /// call on it must have. `None` when the declaration is not a callable
    /// with a first parameter. A function's head is recorded at registration,
    /// so it is known even before its body is linked; a module value takes its
    /// head from its linked function type.
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
            let path = crate::model::join_path(&path, false);
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

    /// Every `pub` declaration of a name in another namespace.
    fn public_lookup(&self, namespace: NamespaceId, name: &str) -> Vec<DeclarationId> {
        self.namespaces[namespace.0]
            .get(name)
            .iter()
            .copied()
            .filter(|declaration| self.skeletons[declaration.0].public)
            .collect()
    }

    fn namespace_at(&self, path: &[String]) -> Option<NamespaceId> {
        namespace_at(&self.namespaces, path)
    }
}
