//! Namespace paths: walking a qualified path, resolving a `use` target, and
//! reading the names a bare name sees.
//!
//! Each `use` of a namespace is resolved once, when the builder indexes what
//! the `use`s deliver, and the reads below take a target from that index
//! rather than resolving the path again.

use crate::compiler::compile::load::LoadedProgram;
use crate::compiler::model::{
    DeclarationId, Delivery, NamespaceId, UseTarget, namespace_at, reachable,
};

use super::registration::Builder;

impl Builder {
    /// Index what each namespace's `use`s deliver. The name a path delivers
    /// keys the target it was resolved to, so every repeated read of a `use`'s
    /// effect is one lookup instead of one walk and one resolution. A path is
    /// resolved here and never again: a `use` that reaches a plain function
    /// waits until the function is registered, which is what the second call
    /// of this method covers.
    pub(in crate::compiler::compile::resolve) fn index_uses(&mut self, loaded: &LoadedProgram) {
        for (use_index, used) in loaded.uses.iter().enumerate() {
            let Some(target) = self.resolve_use(&used.path) else {
                continue;
            };
            let (last, _) = used.path.split_last().expect("a `use` path is not empty");
            let symbol = self.intern.intern(last);
            let namespace = self.ensure_namespace(&used.namespace);
            let brought = &mut self.namespaces[namespace.0].brought;
            let entries = brought.entry(symbol).or_default();
            // A use recorded above is never recorded again: the reader takes
            // the first entry that fits, and one target twice is that entry.
            if !entries.iter().any(|entry| entry.target == target) {
                entries.push(Delivery { target, use_index });
            }
        }
    }

    /// Mark the `use` item that delivered a read resolution as used. The
    /// recording happens where the read happens: the reader takes the first
    /// entry that fits, and that entry's `use` is the one the resolution used.
    pub(in crate::compiler::compile::resolve) fn record_delivery(
        &self,
        entries: &[Delivery],
        taken: usize,
    ) {
        self.used_uses.borrow_mut().insert(entries[taken].use_index);
    }

    /// Resolve a `use` path to the one thing it reaches. The path is absolute:
    /// its prefix names a namespace from the root. This is the one resolution
    /// every pass that reads a `use` directly asks for; the index built from
    /// it answers the repeated reads.
    /// Resolve a `use` path to the one thing a use reaches: a namespace, a
    /// type, or a plain function. Any other declaration is no candidate at
    /// all, so the lookup never names one.
    pub(in crate::compiler::compile::resolve) fn resolve_use(
        &self,
        path: &[String],
    ) -> Option<UseTarget> {
        if let Some(namespace) = namespace_at(&self.intern, &self.namespaces, path) {
            return Some(UseTarget::Namespace(namespace));
        }
        let (name, prefix) = path.split_last()?;
        let namespace = namespace_at(&self.intern, &self.namespaces, prefix)?;
        let namespace_table = &self.namespaces[namespace.0];
        if let Some(found) = namespace_table.type_id(&self.intern, name) {
            return Some(UseTarget::Type(found));
        }
        namespace_table
            .get(&self.intern, name)
            .filter(|declaration| self.skeletons[declaration.0].kind.useable())
            .map(UseTarget::Function)
    }

    /// The namespace a qualified path's first segment names, through the
    /// namespaces this one `use`s: the `use` whose last segment is `first` and
    /// whose whole path is a namespace. The index answers with the one
    /// recording of that `use`, so no path is resolved again here. A qualified
    /// name may also start at a tree name, which `namespace_of_path` reads
    /// first.
    pub(in crate::compiler::compile::resolve) fn used_namespace(
        &self,
        namespace: NamespaceId,
        first: &str,
    ) -> Option<NamespaceId> {
        let symbol = self.intern.resolve(first)?;
        let entries = self.namespaces[namespace.0].brought.get(&symbol)?;
        let taken = entries
            .iter()
            .position(|entry| matches!(entry.target, UseTarget::Namespace(_)))?;
        self.record_delivery(entries, taken);
        match entries[taken].target {
            UseTarget::Namespace(base) => Some(base),
            _ => unreachable!("the position names a namespace entry"),
        }
    }

    /// Walk a namespace path from `base`, not from the root.
    pub(in crate::compiler::compile::resolve) fn namespace_at_from(
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
    pub(in crate::compiler::compile::resolve) fn namespace_of_path(
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
    pub(in crate::compiler::compile::resolve) fn bare_declaration(
        &self,
        namespace: NamespaceId,
        name: &str,
    ) -> Option<DeclarationId> {
        let symbol = self.intern.resolve(name)?;
        let namespace_table = &self.namespaces[namespace.0];
        if let Some(found) = namespace_table.names.get(symbol) {
            return Some(found);
        }
        let entries = namespace_table.brought.get(&symbol)?;
        let taken = entries.iter().position(|entry| match entry.target {
            UseTarget::Function(declaration) => {
                let skeleton = &self.skeletons[declaration.0];
                reachable(skeleton.public, skeleton.namespace, namespace)
            }
            _ => false,
        })?;
        self.record_delivery(entries, taken);
        match entries[taken].target {
            UseTarget::Function(declaration) => Some(declaration),
            _ => unreachable!("the position names a function entry"),
        }
    }
}
