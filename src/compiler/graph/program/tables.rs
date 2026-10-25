//! The name registries a namespace keeps: value names, type names, and the
//! method and associated-function pools.
//!
//! Every table keys a name by the `Symbol` the program's one interner gave
//! it, so a name is hashed once when interned and never again, and a symbol
//! resolves back to its spelling through that interner. Value names and type
//! names are different pools: a type and a value may share a spelling, and a
//! lookup reads the pool the position asks for.

use std::collections::HashMap;

use super::super::ids::DeclarationId;
use super::interner::Symbol;
use crate::compiler::graph::TypeHead;

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

    /// Every registered symbol and the declaration it names, in no particular
    /// order. A relation that proves the key against the declaration's name
    /// reads the pair here.
    #[cfg(debug_assertions)]
    pub(crate) fn iter_entries(&self) -> impl Iterator<Item = (Symbol, DeclarationId)> + '_ {
        self.entries
            .iter()
            .map(|(&symbol, &declaration)| (symbol, declaration))
    }
}

/// One namespace's type-name registry: the type declaration each type name
/// holds. Types and values are separate name pools, so a type name and a value
/// name may be the same spelling; a type position reads this table and never
/// the value table. The table is an index over declaration nodes, not a second
/// identity: every entry names the one declaration the type is.
#[derive(Debug, Default)]
pub(crate) struct TypeTable {
    entries: HashMap<Symbol, DeclarationId>,
}

impl TypeTable {
    /// Register a type declaration under a name. Returns the declaration
    /// already registered under it when the name is taken, and registers
    /// nothing then.
    pub(crate) fn insert(&mut self, name: Symbol, id: DeclarationId) -> Result<(), DeclarationId> {
        match self.entries.entry(name) {
            std::collections::hash_map::Entry::Occupied(entry) => Err(*entry.get()),
            std::collections::hash_map::Entry::Vacant(entry) => {
                entry.insert(id);
                Ok(())
            }
        }
    }

    /// The type declaration registered under a name, if any.
    pub(crate) fn get(&self, name: Symbol) -> Option<DeclarationId> {
        self.entries.get(&name).copied()
    }

    /// The registered type declaration ids, in no particular order.
    pub(crate) fn iter(&self) -> impl Iterator<Item = &DeclarationId> {
        self.entries.values()
    }

    /// Every registered symbol and the type declaration it names, in no
    /// particular order. A relation that proves the key against the
    /// declaration's name reads the pair here.
    #[cfg(debug_assertions)]
    pub(crate) fn iter_entries(&self) -> impl Iterator<Item = (Symbol, DeclarationId)> + '_ {
        self.entries
            .iter()
            .map(|(&symbol, &declaration)| (symbol, declaration))
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

    /// Every registered head, symbol, and declaration, in no particular order.
    /// A relation that proves the pool's key and receiver reads the triple
    /// here.
    #[cfg(debug_assertions)]
    pub(crate) fn iter_entries(
        &self,
    ) -> impl Iterator<Item = (TypeHead, Symbol, DeclarationId)> + '_ {
        self.entries.iter().flat_map(|(&head, methods)| {
            methods
                .iter()
                .map(move |(&symbol, &declaration)| (head, symbol, declaration))
        })
    }
}
