//! The program graph: the program, its files and namespaces, and the accessors
//! over them.

use std::collections::HashMap;
use std::path::PathBuf;

use super::super::declarations::{Declaration, TypeDecl, TypeKind};
use super::super::ids::{DeclarationId, FileId, NamespaceId};
use super::namespace_paths::join_path;
use super::tables::{MethodTable, NameTable, TypeTable};
use crate::compiler::model::{TypeHead, TypeId};

/// A whole program and the nodes it owns. It is compile-time only: the runtime
/// runs the lowered bytecode, not this graph.
#[derive(Debug)]
pub(crate) struct Program {
    pub(crate) files: Vec<File>,
    pub(crate) namespaces: Vec<Namespace>,
    pub(crate) declarations: Vec<Declaration>,
    /// The declared types, indexed by `TypeId`.
    pub(crate) types: Vec<TypeDecl>,
    /// The declared absence type: the `std` category's `Option`. One absence
    /// covers an unset environment variable, an absent search index, and a bare
    /// return.
    pub(crate) absence_type: Option<TypeId>,
    /// The runtime's `Vec` type, registered by the compiler into the
    /// `std::dynamic` namespace. Its representation is the runtime's, so the
    /// lowering is the only compiler stage that reads this id.
    pub(crate) vec_type: Option<TypeId>,
    /// The entry function: the entry file's own root `main`.
    pub(crate) entry: DeclarationId,
    /// The module values in dependency order: each is evaluated once at
    /// startup, after the values its initializer reads.
    pub(crate) module_values: Vec<DeclarationId>,
}

impl Program {
    pub(crate) fn declaration(&self, id: DeclarationId) -> &Declaration {
        &self.declarations[id.0]
    }

    pub(crate) fn declaration_mut(&mut self, id: DeclarationId) -> &mut Declaration {
        &mut self.declarations[id.0]
    }

    /// A declared type by its id.
    pub(crate) fn type_decl(&self, id: TypeId) -> &TypeDecl {
        &self.types[id.0]
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

    /// Whether `id` is the declared absence type.
    pub(crate) fn is_absence_type(&self, id: TypeId) -> bool {
        self.absence_type == Some(id)
    }

    /// The variant index of the absence within the declared absence type: the
    /// payloadless variant (`none`). `None` for any other type.
    pub(crate) fn absence_variant(&self, id: TypeId) -> Option<usize> {
        if !self.is_absence_type(id) {
            return None;
        }
        match &self.type_decl(id).kind {
            TypeKind::Enum(variants) => variants
                .iter()
                .position(|variant| variant.payload.is_empty()),
            TypeKind::Rec(_) => None,
        }
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
    /// `::`.
    pub(crate) fn display(&self, id: DeclarationId) -> String {
        let declaration = self.declaration(id);
        if declaration.owner.is_some() {
            return declaration.name.clone();
        }
        let mut path = self.namespace_path(declaration.namespace);
        path.push(declaration.name.clone());
        join_path(&path)
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

/// One loaded file and the declarations it holds. A file may span several
/// namespaces through `mod` blocks, so it keeps no single namespace.
#[derive(Debug)]
pub(crate) struct File {
    pub(crate) path: PathBuf,
    pub(crate) declarations: Vec<DeclarationId>,
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
    pub(crate) children: HashMap<String, NamespaceId>,
    pub(crate) names: NameTable,
    pub(crate) methods: MethodTable,
    /// The associated functions declared here, keyed by the type they are
    /// filed under and their name. An associated function is reached as
    /// `Type.name(...)`, where a method is reached as `value.name(...)`.
    pub(crate) associates: MethodTable,
    pub(crate) types: TypeTable,
    /// The `use` paths written in this namespace, whose public names it sees.
    pub(crate) uses: Vec<Vec<String>>,
}

impl Namespace {
    /// Every namespace-level declaration of this name in this namespace, in
    /// registration order. A bare name resolves through this.
    pub(crate) fn get(&self, name: &str) -> &[DeclarationId] {
        self.names.get(name)
    }

    /// The method declared here for a receiver head and a name, if any. A
    /// method call resolves through this.
    pub(crate) fn method(&self, head: TypeHead, name: &str) -> Option<DeclarationId> {
        self.methods.get(head, name)
    }

    /// The associated function declared here for a type head and a name, if
    /// any. A `Type.name(...)` call resolves through this.
    pub(crate) fn associate(&self, head: TypeHead, name: &str) -> Option<DeclarationId> {
        self.associates.get(head, name)
    }

    /// A type declared here, if any. A type annotation resolves through this.
    pub(crate) fn type_id(&self, name: &str) -> Option<TypeId> {
        self.types.get(name)
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
