//! Walking and rendering namespace paths.

use super::super::ids::NamespaceId;
use super::graph::{Namespace, Program};

impl Program {
    /// The namespace path, root first.
    pub(crate) fn namespace_path(&self, id: NamespaceId) -> Vec<String> {
        namespace_path(&self.namespaces, id)
    }

    /// The namespace at a path, when one exists.
    pub(crate) fn namespace_at(&self, path: &[String]) -> Option<NamespaceId> {
        namespace_at(&self.namespaces, path)
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
/// and the linker both look namespaces up through this function.
pub(crate) fn namespace_at(namespaces: &[Namespace], path: &[String]) -> Option<NamespaceId> {
    let mut current = NamespaceId(0);
    for segment in path {
        current = *namespaces[current.0].children.get(segment)?;
    }
    Some(current)
}
