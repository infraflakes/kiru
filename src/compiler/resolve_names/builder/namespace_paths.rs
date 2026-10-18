//! Namespace paths: walking a qualified path and reading the names a bare name
//! sees.

use crate::compiler::model::{DeclarationId, NamespaceId, namespace_at, reachable};

use super::registration::Builder;

impl Builder {
    /// The namespace a qualified path's first segment names, through the
    /// namespaces this one `use`s: the `use` whose last segment is `first` and
    /// whose whole path is a namespace. A qualified name may also start at a
    /// tree name, which `namespace_of_path` reads first.
    pub(in crate::compiler::resolve_names) fn used_namespace(
        &self,
        namespace: NamespaceId,
        first: &str,
    ) -> Option<NamespaceId> {
        for used in &self.namespaces[namespace.0].uses {
            if used.last().map(String::as_str) == Some(first)
                && let Some(base) = namespace_at(&self.namespaces, used)
            {
                return Some(base);
            }
        }
        None
    }

    /// The namespace a name-`use` reaches: the path's last segment is `name`
    /// and the rest names a namespace. `None` when the use names a namespace
    /// (its whole path resolves) or when the path reaches no name.
    pub(in crate::compiler::resolve_names) fn name_use(
        &self,
        used: &[String],
        name: &str,
    ) -> Option<NamespaceId> {
        let (last, segments) = used.split_last()?;
        if last != name || segments.is_empty() {
            return None;
        }
        namespace_at(&self.namespaces, segments)
    }

    /// Walk a namespace path from `base`, not from the root.
    pub(in crate::compiler::resolve_names) fn namespace_at_from(
        &self,
        base: NamespaceId,
        path: &[String],
    ) -> Option<NamespaceId> {
        let mut current = base;
        for segment in path {
            current = *self.namespaces[current.0].children.get(segment)?;
        }
        Some(current)
    }

    /// The namespace a path's prefix names. The first segment is a tree name
    /// (`std` or `root`), so the whole prefix walks from the root; otherwise
    /// the first segment is the qualifier a namespace-`use` shortens.
    pub(in crate::compiler::resolve_names) fn namespace_of_path(
        &self,
        namespace: NamespaceId,
        prefix: &[String],
    ) -> Option<NamespaceId> {
        let first = prefix.first()?;
        if let Some(base) = namespace_at(&self.namespaces, std::slice::from_ref(first)) {
            return self.namespace_at_from(base, &prefix[1..]);
        }
        let base = self.used_namespace(namespace, first)?;
        self.namespace_at_from(base, &prefix[1..])
    }

    /// Every declaration a bare name sees: the current namespace's own, then
    /// the declarations a name-`use` brings in. A namespace `use` does not
    /// bring its names bare, and a filed function is not a name.
    pub(in crate::compiler::resolve_names) fn bare_declarations(
        &self,
        namespace: NamespaceId,
        name: &str,
    ) -> Vec<DeclarationId> {
        let mut found: Vec<DeclarationId> = self.namespaces[namespace.0].get(name).to_vec();
        for used in &self.namespaces[namespace.0].uses {
            let Some(base) = self.name_use(used, name) else {
                continue;
            };
            for declaration in self.namespaces[base.0].get(name) {
                if reachable(self.skeletons[declaration.0].public, base, namespace)
                    && !found.contains(declaration)
                {
                    found.push(*declaration);
                }
            }
        }
        found
    }
}
