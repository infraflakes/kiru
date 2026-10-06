//! Resolving a name into an edge.
//!
//! A bare value reads a local first, then climbs outward from the current
//! namespace to the root. A rooted or qualified path names a namespace
//! directly. A function cannot reference itself and a forward reference is an
//! error, so a function can only call functions declared before it.

use crate::compiler::{Diagnostic, function_used_as_value, value_called};
use crate::model::{DeclarationId, NamespaceId, Registry, join_path, namespace_at};
use crate::syntax::Span;

use super::super::register_declarations::Builder;
use super::scopes::Context;

impl Builder {
    /// Resolve a name to a node: a local binding (a value only), then the
    /// namespace registry `registry` names. A rooted path starts at the root
    /// namespace; a bare single segment climbs outward. A function cannot
    /// reference itself and a forward reference is an error, so a function
    /// can only call functions declared before it.
    pub(super) fn resolve(
        &self,
        context: &Context,
        path: &[String],
        root: bool,
        span: Span,
        registry: Registry,
    ) -> Result<DeclarationId, Diagnostic> {
        let reported = &self.files[context.file.0].path;
        // A bare value reads a local first; a local never names a function.
        if !root
            && path.len() == 1
            && matches!(registry, Registry::Value)
            && let Some(found) = context.scopes.find(&path[0])
        {
            return Ok(found);
        }

        let display = join_path(path, root);
        let Some(found) = self.search(context, path, root, registry)? else {
            return Err(self.unknown_name(context, path, root, span, registry));
        };
        if found == context.current {
            let message = if context.current_is_function {
                "a function cannot reference itself"
            } else {
                "a value cannot reference itself"
            };
            return Err(Diagnostic::new(reported, span, message));
        }
        let skeleton = &self.skeletons[found.0];
        if skeleton.order > context.order {
            return Err(Diagnostic::new(
                reported,
                span,
                format!("`{display}` is declared after this point"),
            ));
        }
        Ok(found)
    }

    /// Search one registry for a path: climb outward from the current
    /// namespace for a bare single segment, otherwise start at the root
    /// namespace the path names.
    fn search(
        &self,
        context: &Context,
        path: &[String],
        root: bool,
        registry: Registry,
    ) -> Result<Option<DeclarationId>, Diagnostic> {
        let (name, namespace_segments) = path
            .split_last()
            .expect("a name path has at least one segment");
        if !root && namespace_segments.is_empty() {
            let mut namespace = Some(context.namespace);
            while let Some(current) = namespace {
                if let Some(found) = self.registry_lookup(current, name, registry) {
                    return Ok(Some(found));
                }
                namespace = self.namespaces[current.0].parent;
            }
            return Ok(None);
        }
        let namespace = if namespace_segments.is_empty() {
            NamespaceId(0)
        } else {
            match self.namespace_at(namespace_segments) {
                Some(namespace) => namespace,
                None => return Ok(None),
            }
        };
        Ok(self.registry_lookup(namespace, name, registry))
    }

    /// The diagnostic for a name no registry holds, with a hint when the
    /// other registry of the same search scope does hold it.
    fn unknown_name(
        &self,
        context: &Context,
        path: &[String],
        root: bool,
        span: Span,
        registry: Registry,
    ) -> Diagnostic {
        let other_registry = registry.other();
        let display = join_path(path, root);
        let message = match self.search(context, path, root, other_registry) {
            Ok(Some(_)) => match registry {
                Registry::Value => function_used_as_value(&display),
                Registry::Function => value_called(&display),
            },
            Ok(None) => format!("unknown name `{display}`"),
            Err(diagnostic) => return diagnostic,
        };
        Diagnostic::new(&self.files[context.file.0].path, span, message)
    }

    /// Look one name up in one registry of one namespace.
    fn registry_lookup(
        &self,
        namespace: NamespaceId,
        name: &str,
        registry: Registry,
    ) -> Option<DeclarationId> {
        self.namespaces[namespace.0].lookup(registry, name)
    }

    fn namespace_at(&self, path: &[String]) -> Option<NamespaceId> {
        namespace_at(&self.namespaces, path)
    }
}
