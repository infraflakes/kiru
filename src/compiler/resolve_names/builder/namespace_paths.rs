//! Namespace paths: walking a qualified path, resolving a `use` target, and
//! reading the names a bare name sees.
//!
//! Each `use` of a namespace is resolved once, when the builder indexes what
//! the `use`s deliver, and the reads below take a target from that index
//! rather than resolving the path again.

use crate::compiler::load_files::LoadedProgram;
use crate::compiler::model::{DeclarationId, NamespaceId, UseTarget, namespace_at, reachable};

use super::registration::Builder;

/// Why a `use` path reaches no target.
pub(in crate::compiler::resolve_names) enum UseMiss {
    /// The path ends at a namespace-level declaration a use may not name.
    Declaration,
    /// The path names nothing.
    Nothing,
}

impl Builder {
    /// Index what each namespace's `use`s deliver. The name a path delivers
    /// keys the target it was resolved to, so every repeated read of a `use`'s
    /// effect is one lookup instead of one walk and one resolution. A path is
    /// resolved here and never again: a `use` that reaches a plain function
    /// waits until the function is registered, which is what the second call
    /// of this method covers.
    pub(in crate::compiler::resolve_names) fn index_uses(&mut self, loaded: &LoadedProgram) {
        for used in &loaded.uses {
            let Ok(target) = self.resolve_use(&used.path) else {
                continue;
            };
            let (last, _) = used.path.split_last().expect("a `use` path is not empty");
            let symbol = self.intern.intern(last);
            let namespace = self.ensure_namespace(&used.namespace);
            let brought = &mut self.namespaces[namespace.0].brought;
            let entries = brought.entry(symbol).or_default();
            // A use recorded above is never recorded again: the reader takes
            // the first entry that fits, and one target twice is that entry.
            if !entries.contains(&target) {
                entries.push(target);
            }
        }
    }

    /// Resolve a `use` path to the one thing it reaches. The path is absolute:
    /// its prefix names a namespace from the root. This is the one resolution
    /// every pass that reads a `use` directly asks for; the index built from
    /// it answers the repeated reads.
    pub(in crate::compiler::resolve_names) fn resolve_use(
        &self,
        path: &[String],
    ) -> Result<UseTarget, UseMiss> {
        if let Some(namespace) = namespace_at(&self.intern, &self.namespaces, path) {
            return Ok(UseTarget::Namespace(namespace));
        }
        let Some((name, prefix)) = path.split_last() else {
            return Err(UseMiss::Nothing);
        };
        if prefix.is_empty() {
            return Err(UseMiss::Nothing);
        }
        let Some(namespace) = namespace_at(&self.intern, &self.namespaces, prefix) else {
            return Err(UseMiss::Nothing);
        };
        let namespace_table = &self.namespaces[namespace.0];
        if let Some(found) = namespace_table.type_id(&self.intern, name) {
            return Ok(UseTarget::Type(found));
        }
        match namespace_table.get(&self.intern, name) {
            Some(declaration) if self.skeletons[declaration.0].kind.useable() => {
                Ok(UseTarget::Function(declaration))
            }
            Some(_) => Err(UseMiss::Declaration),
            None => Err(UseMiss::Nothing),
        }
    }

    /// The namespace a qualified path's first segment names, through the
    /// namespaces this one `use`s: the `use` whose last segment is `first` and
    /// whose whole path is a namespace. The index answers with the one
    /// recording of that `use`, so no path is resolved again here. A qualified
    /// name may also start at a tree name, which `namespace_of_path` reads
    /// first.
    pub(in crate::compiler::resolve_names) fn used_namespace(
        &self,
        namespace: NamespaceId,
        first: &str,
    ) -> Option<NamespaceId> {
        let symbol = self.intern.resolve(first)?;
        self.namespaces[namespace.0]
            .brought
            .get(&symbol)?
            .iter()
            .find_map(|target| match target {
                UseTarget::Namespace(base) => Some(*base),
                _ => None,
            })
    }

    /// Walk a namespace path from `base`, not from the root.
    pub(in crate::compiler::resolve_names) fn namespace_at_from(
        &self,
        base: NamespaceId,
        path: &[String],
    ) -> Option<NamespaceId> {
        let mut current = base;
        for segment in path {
            let symbol = self.intern.resolve(segment)?;
            current = *self.namespaces[current.0].children.get(&symbol)?;
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
        if let Some(base) =
            namespace_at(&self.intern, &self.namespaces, std::slice::from_ref(first))
        {
            return self.namespace_at_from(base, &prefix[1..]);
        }
        let base = self.used_namespace(namespace, first)?;
        self.namespace_at_from(base, &prefix[1..])
    }

    /// The declaration a bare name sees: the current namespace's own, then a
    /// plain function a name-`use` brings in. A namespace `use` does not bring
    /// its names bare, a filed function is not a name, and a type is not a
    /// value.
    pub(in crate::compiler::resolve_names) fn bare_declaration(
        &self,
        namespace: NamespaceId,
        name: &str,
    ) -> Option<DeclarationId> {
        let symbol = self.intern.resolve(name)?;
        let namespace_table = &self.namespaces[namespace.0];
        if let Some(found) = namespace_table.names.get(symbol) {
            return Some(found);
        }
        namespace_table
            .brought
            .get(&symbol)?
            .iter()
            .find_map(|target| match target {
                UseTarget::Function(declaration) => {
                    let skeleton = &self.skeletons[declaration.0];
                    reachable(skeleton.public, skeleton.namespace, namespace)
                        .then_some(*declaration)
                }
                _ => None,
            })
    }
}
