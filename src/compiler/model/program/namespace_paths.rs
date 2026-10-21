//! Walking and rendering namespace paths.

use super::super::ids::NamespaceId;
use super::graph::{Namespace, Program, UseTarget};
use super::interner::Interner;

impl Program {
    /// The namespace at a path, when one exists.
    pub(crate) fn namespace_at(&self, path: &[String]) -> Option<NamespaceId> {
        namespace_at(&self.intern, &self.namespaces, path)
    }

    /// The namespace a written path's prefix names, as it is resolved from
    /// `from`: the first segment is a tree name walked from the root, else the
    /// qualifier a namespace-`use` shortens, and the rest descends the
    /// children. The one resolution a qualified path's prefix gets.
    pub(crate) fn qualified_namespace(
        &self,
        from: NamespaceId,
        segments: &[String],
    ) -> Option<NamespaceId> {
        let (first, rest) = segments.split_first()?;
        if let Some(base) =
            namespace_at(&self.intern, &self.namespaces, std::slice::from_ref(first))
        {
            return self.namespace_children(base, rest);
        }
        let symbol = self.intern.resolve(first)?;
        let entries = self.namespaces[from.0].brought.get(&symbol)?;
        let base = entries.iter().find_map(|entry| match entry.target {
            UseTarget::Namespace(namespace) => Some(namespace),
            _ => None,
        })?;
        self.namespace_children(base, rest)
    }

    /// The namespace the remaining segments descend to, from `base`.
    fn namespace_children(&self, base: NamespaceId, segments: &[String]) -> Option<NamespaceId> {
        let mut current = base;
        for segment in segments {
            let symbol = self.intern.resolve(segment)?;
            current = *self.namespaces[current.0].children.get(&symbol)?;
        }
        Some(current)
    }
}

/// The namespace path, root first, walked over a namespace table. The program
/// model and the linker both derive namespace paths through this function.
pub(crate) fn namespace_path(namespaces: &[Namespace], id: NamespaceId) -> Vec<String> {
    let mut segments = Vec::new();
    let mut current = Some(id);
    while let Some(namespace_id) = current {
        let namespace = &namespaces[namespace_id.0];
        current = namespace.parent;
        if namespace.parent.is_some() {
            segments.push(namespace.name.clone());
        }
    }
    segments.reverse();
    segments
}

/// Join namespace segments with `::`. The program model and the linker both
/// render names through this function.
pub(crate) fn join_path(segments: &[String]) -> String {
    segments.join("::")
}

/// The namespace at a path, walked over a namespace table. The program model
/// and the linker both look namespaces up through this function. A segment
/// keys its namespace's children by the symbol the interner gave it, so a
/// never-interned name ends the walk.
pub(crate) fn namespace_at(
    intern: &Interner,
    namespaces: &[Namespace],
    path: &[String],
) -> Option<NamespaceId> {
    let mut current = NamespaceId(0);
    for segment in path {
        let symbol = intern.resolve(segment)?;
        current = *namespaces[current.0].children.get(&symbol)?;
    }
    Some(current)
}
