//! The program graph: the program, its files and namespaces, and the name
//! registries a namespace keeps.

use std::collections::HashMap;
use std::path::PathBuf;

use super::declarations::{Declaration, TypeDecl, TypeFieldDecl, TypeKind};
use super::ids::{DeclarationId, FileId, NamespaceId};
use crate::model::{Type, TypeHead, TypeId, substitute};

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

    /// The name of a type for a diagnostic: a declared type shows its declared
    /// name and arguments, a parameter shows its name, and every other type
    /// shows its registry name.
    pub(crate) fn type_name(&self, ty: &Type) -> String {
        match ty {
            Type::Named(id, arguments) => {
                let name = &self.type_decl(*id).name;
                if arguments.is_empty() {
                    name.clone()
                } else {
                    let parts: Vec<String> = arguments
                        .iter()
                        .map(|argument| self.type_name(argument))
                        .collect();
                    format!("{name}<{}>", parts.join(", "))
                }
            }
            Type::Parameter(name) => name.clone(),
            Type::Function {
                parameters,
                returns,
            } => {
                let parts: Vec<String> = parameters
                    .iter()
                    .map(|parameter| self.type_name(parameter))
                    .collect();
                // A function with no return type has no `->` in its written
                // form, because it has no return type to write.
                match returns {
                    Some(returns) => {
                        format!("fn({}) -> {}", parts.join(", "), self.type_name(returns))
                    }
                    None => format!("fn({})", parts.join(", ")),
                }
            }
            other => other.builtin_name().to_owned(),
        }
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

/// One namespace node: the name registry, the method registry, plus the parent
/// and child edges. A name is a name: a function and a value share one
/// namespace, and a name holds several declarations only when they are methods
/// that differ by their receiver's type head.
#[derive(Debug)]
pub(crate) struct Namespace {
    pub(crate) name: String,
    pub(crate) parent: Option<NamespaceId>,
    pub(crate) children: HashMap<String, NamespaceId>,
    pub(crate) names: NameTable,
    pub(crate) methods: MethodTable,
    pub(crate) types: TypeTable,
    /// The `use` paths written in this namespace, whose public names it sees.
    pub(crate) uses: Vec<Vec<String>>,
}

impl Namespace {
    /// Every declaration of this name, in registration order. A bare name
    /// resolves through this; several declarations make the name ambiguous.
    pub(crate) fn get(&self, name: &str) -> &[DeclarationId] {
        self.names.get(name)
    }

    /// The method or constructor declared here for a head and a name, if any.
    /// A method call and a constructor call both resolve through this.
    pub(crate) fn method(&self, head: TypeHead, name: &str) -> Option<DeclarationId> {
        self.methods.get(head, name)
    }

    /// A type declared here, if any. A type annotation resolves through this.
    pub(crate) fn type_id(&self, name: &str) -> Option<TypeId> {
        self.types.get(name)
    }
}

/// One namespace's name registry: every declaration under each name. A name
/// usually holds one declaration. It holds several only when they are methods
/// that differ by their receiver's type head — the family `MethodTable` keys,
/// so no call is ever ambiguous. A bare name with several declarations is
/// ambiguous. The table serializes exactly as the map it holds, so a compiled
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
        for ids in self.entries.values_mut() {
            ids.retain(|id| mapping[id.0].is_some());
            for id in ids.iter_mut() {
                if let Some(mapped) = mapping[id.0] {
                    *id = mapped;
                }
            }
        }
        self.entries.retain(|_, ids| !ids.is_empty());
    }
}

/// One namespace's method registry: the pool that makes a method family
/// callable and a type's constructors reachable. A method is selected by its
/// receiver's type head and a constructor by the head of the type it returns,
/// so a name may repeat once per head. The table maps a head and a name to the
/// declaration: a method is a function whose first parameter has that head, and
/// a constructor is a function with no parameters whose return type has that
/// head. A function with neither (no parameters and no return type) has no head
/// and lives only in the name pool. Unlike a `NameTable` it is not serialized:
/// it is rebuilt from the names at compile time.
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
        for by_name in self.entries.values_mut() {
            by_name.retain(|_, id| mapping[id.0].is_some());
            for id in by_name.values_mut() {
                if let Some(mapped) = mapping[id.0] {
                    *id = mapped;
                }
            }
        }
        self.entries.retain(|_, by_name| !by_name.is_empty());
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

/// The type of a field of a declared `rec` type, substituted with the type's
/// arguments. `None` when the type is not a rec or has no such field. The
/// linker's receiver typing and the checker's field access both read a field
/// through this one lookup.
pub(crate) fn rec_field(
    types: &[TypeDecl],
    id: TypeId,
    arguments: &[Type],
    name: &str,
) -> Option<Type> {
    let declaration = &types[id.0];
    let TypeKind::Rec(fields) = &declaration.kind else {
        return None;
    };
    fields
        .iter()
        .find(|field| field.name == name)
        .map(|field| substitute(&field.ty, &declaration.parameters, arguments))
}

/// The declared field node of a `rec` type, or `None` when the type is not a
/// rec or has no such field. The checker reads a field's visibility through
/// this lookup, the same field `rec_field` types.
pub(crate) fn rec_field_decl<'a>(
    types: &'a [TypeDecl],
    id: TypeId,
    name: &str,
) -> Option<&'a TypeFieldDecl> {
    let TypeKind::Rec(fields) = &types[id.0].kind else {
        return None;
    };
    fields.iter().find(|field| field.name == name)
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
