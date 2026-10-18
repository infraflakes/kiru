//! The name registries a namespace keeps: namespace-level names, the method
//! and associated-function pools, and declared type names.

use std::collections::HashMap;

use super::super::ids::DeclarationId;
use crate::compiler::model::{TypeHead, TypeId};

/// Keep only the ids the mapping keeps, and rewrite each surviving id through
/// it. Both registries that hold declaration ids remap through this one scan.
fn remap_id(id: &mut DeclarationId, mapping: &[Option<DeclarationId>]) -> bool {
    match mapping[id.0] {
        Some(mapped) => {
            *id = mapped;
            true
        }
        None => false,
    }
}

/// A registry's inner collection of declaration ids, remapped in place: keep
/// only the ids the mapping keeps, rewrite each survivor through it, and report
/// whether any id remains. A name table holds one `Vec` per name and a method
/// table one `HashMap` per head, so the two shapes share this one scan.
trait DeclarationIds {
    fn remap(&mut self, mapping: &[Option<DeclarationId>]) -> bool;
}

impl DeclarationIds for Vec<DeclarationId> {
    fn remap(&mut self, mapping: &[Option<DeclarationId>]) -> bool {
        self.retain_mut(|id| remap_id(id, mapping));
        !self.is_empty()
    }
}

impl DeclarationIds for HashMap<String, DeclarationId> {
    fn remap(&mut self, mapping: &[Option<DeclarationId>]) -> bool {
        self.retain(|_, id| remap_id(id, mapping));
        !self.is_empty()
    }
}

/// Keep only the entries whose inner ids the mapping keeps, and rewrite each
/// survivor through it. Retention compacts the declaration table. Both
/// registries that hold declaration ids remap through this one scan.
fn remap_registry<K, I>(entries: &mut HashMap<K, I>, mapping: &[Option<DeclarationId>])
where
    K: Eq + std::hash::Hash,
    I: DeclarationIds,
{
    entries.retain(|_, ids| ids.remap(mapping));
}

/// One namespace's name registry: every namespace-level declaration under each
/// name. A name holds one declaration, so a bare name is never ambiguous
/// within its namespace. A filed function is not here: it is reached through
/// its type. The table serializes exactly as the map it holds, so a compiled
/// binary stores it without a wrapper.
#[derive(Debug, Default)]
pub(crate) struct NameTable {
    entries: HashMap<String, Vec<DeclarationId>>,
}

impl NameTable {
    /// Append a declaration under a name. The caller has already enforced the
    /// duplicate rule against the method pool.
    pub(crate) fn insert(&mut self, name: String, id: DeclarationId) {
        self.entries.entry(name).or_default().push(id);
    }

    /// Every declaration registered under a name, in registration order.
    pub(crate) fn get(&self, name: &str) -> &[DeclarationId] {
        self.entries.get(name).map_or(&[], Vec::as_slice)
    }

    /// Whether a name has any declaration.
    pub(crate) fn contains(&self, name: &str) -> bool {
        self.entries.contains_key(name)
    }

    /// The registered declaration ids, in no particular order.
    pub(crate) fn iter(&self) -> impl Iterator<Item = &DeclarationId> {
        self.entries.values().flatten()
    }

    /// Keep only the entries whose id the mapping keeps, and rewrite each
    /// surviving id through it. Retention compacts the declaration table.
    pub(crate) fn remap(&mut self, mapping: &[Option<DeclarationId>]) {
        remap_registry(&mut self.entries, mapping);
    }
}

/// One namespace's method registry: the pool that makes a method family
/// callable and a type's associated functions reachable. A method is selected
/// by its receiver's type head and an associated function by its type's head,
/// so a name may repeat once per head in each pool. The table maps a head and a
/// name to the declaration: a method is a function in an `impl` block whose
/// first parameter has that head, and an associated function is a function in
/// an `impl` block whose first parameter does not. A function outside a block
/// has no head and lives only in the name pool. Unlike a `NameTable` it is not
/// serialized: it is rebuilt from the names at compile time.
#[derive(Debug, Default)]
pub(crate) struct MethodTable {
    entries: HashMap<TypeHead, HashMap<String, DeclarationId>>,
}

impl MethodTable {
    /// Register a method under its receiver head and name. Returns the id
    /// already registered under that pair when it is taken, and registers
    /// nothing then.
    pub(crate) fn insert(
        &mut self,
        head: TypeHead,
        name: String,
        id: DeclarationId,
    ) -> Result<(), DeclarationId> {
        match self.entries.entry(head).or_default().entry(name) {
            std::collections::hash_map::Entry::Occupied(entry) => Err(*entry.get()),
            std::collections::hash_map::Entry::Vacant(entry) => {
                entry.insert(id);
                Ok(())
            }
        }
    }

    /// The method registered under a receiver head and name, if any.
    pub(crate) fn get(&self, head: TypeHead, name: &str) -> Option<DeclarationId> {
        self.entries.get(&head)?.get(name).copied()
    }

    /// The registered method ids, in no particular order.
    pub(crate) fn iter(&self) -> impl Iterator<Item = &DeclarationId> {
        self.entries.values().flat_map(HashMap::values)
    }

    /// Keep only the entries whose id the mapping keeps, and rewrite each
    /// surviving id through it. Retention compacts the declaration table.
    pub(crate) fn remap(&mut self, mapping: &[Option<DeclarationId>]) {
        remap_registry(&mut self.entries, mapping);
    }
}

/// A namespace's type registry: declared type names, unique within it. Types
/// are compile-time only, so unlike a `NameTable` it never needs remapping.
#[derive(Debug, Default)]
pub(crate) struct TypeTable {
    entries: HashMap<String, TypeId>,
}

impl TypeTable {
    /// Register a type name. Returns the id already registered under it when
    /// the name is taken, and registers nothing then.
    pub(crate) fn insert(&mut self, name: String, id: TypeId) -> Result<(), TypeId> {
        match self.entries.entry(name) {
            std::collections::hash_map::Entry::Occupied(entry) => Err(*entry.get()),
            std::collections::hash_map::Entry::Vacant(entry) => {
                entry.insert(id);
                Ok(())
            }
        }
    }

    /// The type registered under a name, if any.
    pub(crate) fn get(&self, name: &str) -> Option<TypeId> {
        self.entries.get(name).copied()
    }
}
