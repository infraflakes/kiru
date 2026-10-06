//! The program graph: the program, its files and namespaces, and the name
//! registries a namespace keeps.

use std::collections::HashMap;
use std::path::PathBuf;

use super::declarations::Declaration;
use super::ids::{DeclarationId, FileId, NamespaceId};

/// A whole program and the nodes it owns. It is compile-time only: the runtime
/// runs the lowered bytecode, not this graph.
#[derive(Debug)]
pub(crate) struct Program {
    pub(crate) files: Vec<File>,
    pub(crate) namespaces: Vec<Namespace>,
    pub(crate) declarations: Vec<Declaration>,
    /// The entry function: the entry file's own root `main`.
    pub(crate) entry: DeclarationId,
}

impl Program {
    pub(crate) fn declaration(&self, id: DeclarationId) -> &Declaration {
        &self.declarations[id.0]
    }

    pub(crate) fn declaration_mut(&mut self, id: DeclarationId) -> &mut Declaration {
        &mut self.declarations[id.0]
    }

    pub(crate) fn file(&self, id: FileId) -> &File {
        &self.files[id.0]
    }

    pub(crate) fn namespace(&self, id: NamespaceId) -> &Namespace {
        &self.namespaces[id.0]
    }

    pub(crate) fn root(&self) -> NamespaceId {
        NamespaceId(0)
    }

    /// The namespace path, root first.
    pub(crate) fn namespace_path(&self, id: NamespaceId) -> Vec<String> {
        namespace_path(&self.namespaces, id)
    }

    /// The namespace at a path, when one exists.
    pub(crate) fn namespace_at(&self, path: &[String]) -> Option<NamespaceId> {
        namespace_at(&self.namespaces, path)
    }

    /// The file a declaration came from. A native and a local binding have no
    /// file, so only a caller holding a file declaration asks for this.
    pub(crate) fn file_of(&self, id: DeclarationId) -> FileId {
        self.declaration(id)
            .file
            .expect("a file declaration has a file")
    }

    /// The display name of a declaration: a local binding shows its bare name,
    /// and a file declaration shows its namespace path and name joined with
    /// `::`, with no leading `::`.
    pub(crate) fn display(&self, id: DeclarationId) -> String {
        let declaration = self.declaration(id);
        if declaration.owner.is_some() {
            return declaration.name.clone();
        }
        let mut path = self.namespace_path(declaration.namespace);
        path.push(declaration.name.clone());
        join_path(&path, false)
    }
}

/// Where a loaded file came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Origin {
    /// A file seeded from the compiler's own sources.
    Embedded,
    /// A file read from the filesystem.
    File,
}

/// One loaded file and the nodes it owns.
#[derive(Debug)]
pub(crate) struct File {
    pub(crate) path: PathBuf,
    pub(crate) namespace: NamespaceId,
    pub(crate) imports: Vec<FileId>,
    pub(crate) declarations: Vec<DeclarationId>,
}

/// One namespace node: the value and function registries, plus the parent and
/// child edges. Values are unique by name and functions are unique by name,
/// and a function may share a name with a value.
#[derive(Debug)]
pub(crate) struct Namespace {
    pub(crate) name: String,
    pub(crate) parent: Option<NamespaceId>,
    pub(crate) children: HashMap<String, NamespaceId>,
    pub(crate) values: NameTable,
    pub(crate) functions: NameTable,
}

impl Namespace {
    /// A value declared here, if any. A bare name resolves through this.
    pub(crate) fn value(&self, name: &str) -> Option<DeclarationId> {
        self.values.get(name)
    }

    /// A function declared here, if any. A call resolves through this.
    pub(crate) fn function(&self, name: &str) -> Option<DeclarationId> {
        self.functions.get(name)
    }

    /// One name looked up in one registry of this namespace.
    pub(crate) fn lookup(&self, registry: Registry, name: &str) -> Option<DeclarationId> {
        match registry {
            Registry::Value => self.value(name),
            Registry::Function => self.function(name),
        }
    }

    /// One registry of this namespace, for registering a declaration.
    pub(crate) fn table_mut(&mut self, registry: Registry) -> &mut NameTable {
        match registry {
            Registry::Value => &mut self.values,
            Registry::Function => &mut self.functions,
        }
    }
}

/// Which registry of a namespace a name lives in.
#[derive(Clone, Copy)]
pub(crate) enum Registry {
    Value,
    Function,
}

impl Registry {
    /// The other registry of the same namespace.
    pub(crate) fn other(self) -> Registry {
        match self {
            Registry::Value => Registry::Function,
            Registry::Function => Registry::Value,
        }
    }
}

/// One namespace registry: names that are unique within a namespace. A
/// namespace keeps one for values and one for functions, so the duplicate
/// rule lives here once. The table serializes exactly as the map it holds,
/// so a compiled binary stores it without a wrapper.
#[derive(Debug, Default)]
pub(crate) struct NameTable {
    entries: HashMap<String, DeclarationId>,
}

impl NameTable {
    /// Register a name. Returns the id already registered under it when the
    /// name is taken, and registers nothing then.
    pub(crate) fn insert(&mut self, name: String, id: DeclarationId) -> Result<(), DeclarationId> {
        match self.entries.entry(name) {
            std::collections::hash_map::Entry::Occupied(entry) => Err(*entry.get()),
            std::collections::hash_map::Entry::Vacant(entry) => {
                entry.insert(id);
                Ok(())
            }
        }
    }

    /// The declaration registered under a name, if any.
    pub(crate) fn get(&self, name: &str) -> Option<DeclarationId> {
        self.entries.get(name).copied()
    }

    /// The registered declaration ids, in no particular order.
    pub(crate) fn iter(&self) -> impl Iterator<Item = &DeclarationId> {
        self.entries.values()
    }

    /// Keep only the entries whose id the mapping keeps, and rewrite each
    /// surviving id through it. Retention compacts the declaration table.
    pub(crate) fn remap(&mut self, mapping: &[Option<DeclarationId>]) {
        self.entries.retain(|_, id| mapping[id.0].is_some());
        for id in self.entries.values_mut() {
            if let Some(mapped) = mapping[id.0] {
                *id = mapped;
            }
        }
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

/// Join namespace segments with `::`, with an optional leading `::` for a
/// rooted path. The program model and the linker both render names through
/// this function.
pub(crate) fn join_path(segments: &[String], root: bool) -> String {
    let joined = segments.join("::");
    if root { format!("::{joined}") } else { joined }
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
