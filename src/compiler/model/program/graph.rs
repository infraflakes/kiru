//! The program graph: the program, its files and namespaces, and the accessors
//! over them.

use std::collections::HashMap;
use std::path::PathBuf;

use super::super::declarations::{Declaration, TypeUse};
use super::super::ids::{DeclarationId, FileId, NamespaceId};
use super::interner::{Interner, Symbol};
use super::tables::{MethodTable, NameTable, TypeTable};

/// A whole program and the nodes it owns. It is compile-time only: the runtime
/// runs the lowered bytecode, not this graph.
#[derive(Debug)]
pub(crate) struct Program {
    pub(crate) files: Vec<File>,
    pub(crate) namespaces: Vec<Namespace>,
    pub(crate) declarations: Vec<Declaration>,
    /// The declaration the runtime represents itself, when the program has
    /// one. The compiler registers it once; a pass reads this fact rather
    /// than scanning declarations for the runtime marker.
    pub(crate) list_type: Option<DeclarationId>,
    /// The module values in dependency order: each is evaluated once at
    /// startup, after the values its initializer reads.
    pub(crate) module_values: Vec<DeclarationId>,
    /// The one interner every name-table key came from. A table key is a
    /// `Symbol`, so the spelling it names resolves through this.
    pub(crate) intern: Interner,
    /// Every written type name, in the order the resolution met them.
    pub(crate) type_uses: Vec<TypeUse>,
}

impl Program {
    pub(crate) fn declaration(&self, id: DeclarationId) -> &Declaration {
        &self.declarations[id.0]
    }

    pub(crate) fn namespace(&self, id: NamespaceId) -> &Namespace {
        &self.namespaces[id.0]
    }

    pub(crate) fn root(&self) -> NamespaceId {
        NamespaceId(0)
    }

    /// The type the runtime represents itself, when the program has one. The
    /// compiler registers it once and stores the fact; no pass scans the
    /// declaration table for the runtime marker.
    pub(crate) fn list_type(&self) -> Option<DeclarationId> {
        self.list_type
    }

    /// Whether a path belongs to the program tree. A project file is the
    /// program's; an embedded library file is not. A path the program's files
    /// do not name is a project fact (a manifest or a load error), so it is
    /// treated as the program's.
    pub(crate) fn is_program_path(&self, path: &std::path::Path) -> bool {
        self.files
            .iter()
            .find(|file| file.path == path)
            .is_none_or(|file| file.origin == FileOrigin::Program)
    }

    /// The type declaration a name denotes in a namespace, when the name is a
    /// type. Type names and value names are different pools, so a type name
    /// may share its spelling with a value name; this reads the type pool and
    /// never the value pool. This is a test query; the resolver reads the same
    /// relation through its builder.
    #[cfg(test)]
    pub(crate) fn type_id(&self, namespace: NamespaceId, name: &str) -> Option<DeclarationId> {
        self.namespace(namespace).type_id(&self.intern, name)
    }

    /// The file a declaration came from. A native and a local binding have no
    /// file, so only a caller holding a file declaration asks for this.
    pub(crate) fn file_of(&self, id: DeclarationId) -> FileId {
        self.declaration(id)
            .file
            .expect("a file declaration has a file")
    }
}

/// Why a file is in the program. A project file comes from the manifest's
/// tree; an embedded file comes from the library table. The origin is the
/// fact a warning reads; no pass decides it from a path.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum FileOrigin {
    Program,
    Embedded,
}

/// One loaded file and the declarations it holds. A file belongs to exactly
/// one namespace, the one its directory names; the namespace is recorded
/// here, and where each declaration joins it is recorded on the declaration.
#[derive(Debug)]
pub(crate) struct File {
    pub(crate) path: PathBuf,
    pub(crate) origin: FileOrigin,
    pub(crate) declarations: Vec<DeclarationId>,
}

/// What a `use` path reaches. A use shortens a path to exactly one of these
/// three things: a namespace, a type, or a plain function. A filed function is
/// reached through its type, so it is never a use target; a module value is
/// never a use target.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum UseTarget {
    Namespace(NamespaceId),
    Type(DeclarationId),
    Function(DeclarationId),
}

/// One delivery a namespace's `use`s recorded: the target the delivered name
/// reaches, the `use` item that delivered it, and whether a resolution took
/// it. The index names the item in the loaded program's stream, so the
/// delivery status is derived from the edge rather than from a second set of
/// read use indices.
#[derive(Debug)]
pub(crate) struct Delivery {
    pub(crate) target: UseTarget,
    pub(crate) use_index: usize,
    pub(crate) taken: std::cell::Cell<bool>,
}

/// One namespace node: its value-name pool, its type-name pool, the method and
/// associated-function registries, plus the parent and child edges. Type names
/// and value names are different pools, so a declared type and a function or
/// value may share a spelling; a type position reads `types` and a value
/// position reads `names`. A function filed under a type by an `impl` block is
/// not a name at all: it lives in the method or associated-function pool,
/// keyed by its type head.
#[derive(Debug)]
pub(crate) struct Namespace {
    pub(crate) name: String,
    pub(crate) parent: Option<NamespaceId>,
    pub(crate) children: HashMap<Symbol, NamespaceId>,
    /// The namespace-level value and function names. A type declaration is
    /// never registered here.
    pub(crate) names: NameTable,
    /// The declared type names. A type declaration is registered here and in
    /// no value pool.
    pub(crate) types: TypeTable,
    pub(crate) methods: MethodTable,
    /// The associated functions declared here, keyed by the type they are
    /// filed under and their name. An associated function is reached as
    /// `Type.name(...)`, where a method is reached as `value.name(...)`.
    pub(crate) associates: MethodTable,
    /// What each `use` this namespace wrote reaches, resolved once, keyed by
    /// the name it delivers. Two `use`s may deliver one name, so the entries
    /// keep the file's order and the reader takes the first that fits.
    pub(crate) brought: HashMap<Symbol, Vec<Delivery>>,
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

    /// A type declared here, if any. A type annotation resolves through this
    /// pool and never through the value names.
    pub(crate) fn type_id(&self, intern: &Interner, name: &str) -> Option<DeclarationId> {
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
