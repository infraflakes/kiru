//! The name registries a namespace keeps: namespace-level names, the method
//! and associated-function pools, and declared type names.
//!
//! Every table keys a name by the `Symbol` the program's one interner gave
//! it, so a name is hashed once when interned and never again, and a symbol
//! resolves back to its spelling through that interner.

use std::collections::HashMap;

use super::super::ids::DeclarationId;
use super::interner::Symbol;
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

/// One namespace's name registry: the one declaration each namespace-level
/// name holds. A name is unique inside its namespace, so a bare name is never
/// ambiguous. A filed function is not here: it is reached through its type. A
/// symbol keys this table only when its name was interned, so a table read
/// resolves the name through the interner and none resolves on `None`.
#[derive(Debug, Default)]
pub(crate) struct NameTable {
    entries: HashMap<Symbol, DeclarationId>,
}

impl NameTable {
    /// Register a declaration under a name. The caller has already enforced
    /// the duplicate rule against the name and the method pools.
    pub(crate) fn insert(&mut self, name: Symbol, id: DeclarationId) {
        self.entries.insert(name, id);
    }

    /// The declaration registered under a name, if any.
    pub(crate) fn get(&self, name: Symbol) -> Option<DeclarationId> {
        self.entries.get(&name).copied()
    }

    /// The registered declaration ids, in no particular order.
    pub(crate) fn iter(&self) -> impl Iterator<Item = &DeclarationId> {
        self.entries.values()
    }

    /// Keep only the entries whose id the mapping keeps, and rewrite each
    /// surviving id through it. Retention compacts the declaration table.
    pub(crate) fn remap(&mut self, mapping: &[Option<DeclarationId>]) {
        self.entries.retain(|_, id| remap_id(id, mapping));
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
    entries: HashMap<TypeHead, HashMap<Symbol, DeclarationId>>,
}

impl MethodTable {
    /// Register a method under its receiver head and name. Returns the id
    /// already registered under that pair when it is taken, and registers
    /// nothing then.
    pub(crate) fn insert(
        &mut self,
        head: TypeHead,
        name: Symbol,
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
    pub(crate) fn get(&self, head: TypeHead, name: Symbol) -> Option<DeclarationId> {
        self.entries.get(&head)?.get(&name).copied()
    }

    /// The registered method ids, in no particular order.
    pub(crate) fn iter(&self) -> impl Iterator<Item = &DeclarationId> {
        self.entries.values().flat_map(HashMap::values)
    }

    /// Keep only the entries whose id the mapping keeps, and rewrite each
    /// surviving id through it. Retention compacts the declaration table.
    pub(crate) fn remap(&mut self, mapping: &[Option<DeclarationId>]) {
        self.entries.retain(|_, methods| {
            methods.retain(|_, id| remap_id(id, mapping));
            !methods.is_empty()
        });
    }
}

/// A namespace's type registry: declared type names, unique within it. Types
/// are compile-time only, so unlike a `NameTable` it never needs remapping.
#[derive(Debug, Default)]
pub(crate) struct TypeTable {
    entries: HashMap<Symbol, TypeId>,
}

impl TypeTable {
    /// Register a type name. Returns the id already registered under it when
    /// the name is taken, and registers nothing then.
    pub(crate) fn insert(&mut self, name: Symbol, id: TypeId) -> Result<(), TypeId> {
        match self.entries.entry(name) {
            std::collections::hash_map::Entry::Occupied(entry) => Err(*entry.get()),
            std::collections::hash_map::Entry::Vacant(entry) => {
                entry.insert(id);
                Ok(())
            }
        }
    }

    /// The type registered under a name, if any.
    pub(crate) fn get(&self, name: Symbol) -> Option<TypeId> {
        self.entries.get(&name).copied()
    }
}
