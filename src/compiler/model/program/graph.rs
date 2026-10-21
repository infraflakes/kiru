//! The program graph: the program, its files and namespaces, and the accessors
//! over them.

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use super::super::declarations::{Declaration, TypeDecl};
use super::super::ids::{DeclarationId, FileId, NamespaceId};
use super::interner::{Interner, Symbol};
use super::tables::{MethodTable, NameTable, TypeTable};
use crate::compiler::model::{TypeId, list_type};

/// A whole program and the nodes it owns. It is compile-time only: the runtime
/// runs the lowered bytecode, not this graph.
#[derive(Debug)]
pub(crate) struct Program {
    pub(crate) files: Vec<File>,
    pub(crate) namespaces: Vec<Namespace>,
    pub(crate) declarations: Vec<Declaration>,
    /// The declared types, indexed by `TypeId`.
    pub(crate) types: Vec<TypeDecl>,
    /// The entry function: the entry file's own root `main`.
    pub(crate) entry: Option<DeclarationId>,
    /// The module values in dependency order: each is evaluated once at
    /// startup, after the values its initializer reads.
    pub(crate) module_values: Vec<DeclarationId>,
    /// The one interner every name-table key came from. A table key is a
    /// `Symbol`, so the spelling it names resolves through this.
    pub(crate) intern: Interner,
}

impl Program {
    pub(crate) fn declaration(&self, id: DeclarationId) -> &Declaration {
        &self.declarations[id.0]
    }

    pub(crate) fn declaration_mut(&mut self, id: DeclarationId) -> &mut Declaration {
        &mut self.declarations[id.0]
    }

    /// A declared type by its id.
    pub(crate) fn namespace(&self, id: NamespaceId) -> &Namespace {
        &self.namespaces[id.0]
    }

    pub(crate) fn root(&self) -> NamespaceId {
        NamespaceId(0)
    }

    /// The type the runtime represents itself, when the program has one: the
    /// declaration that carries the runtime's marker. Every stage that needs
    /// the runtime list reads its id from that marker rather than carrying it.
    pub(crate) fn list_type(&self) -> Option<TypeId> {
        list_type(&self.types)
    }

    /// The file a declaration came from. A native and a local binding have no
    /// file, so only a caller holding a file declaration asks for this.
    pub(crate) fn file_of(&self, id: DeclarationId) -> FileId {
        self.declaration(id)
            .file
            .expect("a file declaration has a file")
    }
}

/// One loaded file and the declarations it holds. A file belongs to exactly
/// one namespace, the one its directory names; the namespace is recorded
/// here, and where each declaration joins it is recorded on the declaration.
#[derive(Debug)]
pub(crate) struct File {
    pub(crate) path: PathBuf,
    pub(crate) declarations: Vec<DeclarationId>,
}

/// What a `use` path reaches. A use shortens a path to exactly one of these
/// three things: a namespace, a type, or a plain function. A filed function is
/// reached through its type, so it is never a use target; a module value is
/// never a use target.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum UseTarget {
    Namespace(NamespaceId),
    Type(TypeId),
    Function(DeclarationId),
}

/// One delivery a namespace's `use`s recorded: the target the delivered name
/// reaches and the `use` item that delivered it. The index names the item in
/// the loaded program's stream, which is how the unused-`use` fact reads back
/// which deliveries a resolution actually took.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Delivery {
    pub(crate) target: UseTarget,
    pub(crate) use_index: usize,
}

/// One namespace node: the name registry, the method and associated-function
/// registries, plus the parent and child edges. A name is a name: a function
/// and a value share one namespace. A function filed under a type by an `impl`
/// block is not a name at all: it lives in the method or associated-function
/// pool, keyed by its type head.
#[derive(Debug)]
pub(crate) struct Namespace {
    pub(crate) name: String,
    pub(crate) parent: Option<NamespaceId>,
    pub(crate) children: HashMap<Symbol, NamespaceId>,
    pub(crate) names: NameTable,
    pub(crate) methods: MethodTable,
    /// The associated functions declared here, keyed by the type they are
    /// filed under and their name. An associated function is reached as
    /// `Type.name(...)`, where a method is reached as `value.name(...)`.
    pub(crate) associates: MethodTable,
    pub(crate) types: TypeTable,
    /// What each `use` this namespace wrote reaches, resolved once, keyed by
    /// the name it delivers. Two `use`s may deliver one name, so the entries
    /// keep the file's order and the reader takes the first that fits.
    pub(crate) brought: HashMap<Symbol, Vec<Delivery>>,
    /// The names filed in any pool of this namespace: a filed function is not
    /// a name, so the one-flat-map read replaces a walk over both pools.
    pub(crate) filed: HashSet<Symbol>,
}

impl Namespace {
    /// The namespace-level declaration of this name in this namespace. A name
    /// holds one declaration, so there is at most one. A bare name resolves
    /// through this.
    pub(crate) fn get(&self, intern: &Interner, name: &str) -> Option<DeclarationId> {
        intern
            .resolve(name)
            .and_then(|symbol| self.names.get(symbol))
    }

    /// A type declared here, if any. A type annotation resolves through this.
    pub(crate) fn type_id(&self, intern: &Interner, name: &str) -> Option<TypeId> {
        intern
            .resolve(name)
            .and_then(|symbol| self.types.get(symbol))
    }
}

/// The index of the first name that repeats one before it, if any. Every
/// duplicate-name rule reports the repeated name through this one scan.
pub(crate) fn first_duplicate<'a>(names: impl Iterator<Item = &'a str>) -> Option<usize> {
    let mut seen: Vec<&'a str> = Vec::new();
    for (index, name) in names.enumerate() {
        if seen.contains(&name) {
            return Some(index);
        }
        seen.push(name);
    }
    None
}
