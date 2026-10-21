//! Registering declaration nodes: reserving types, declaring names, and
//! filling a type's fields.

use std::cell::RefCell;
use std::collections::{HashMap, HashSet};

use crate::compiler::model::{
    Declaration, DeclarationId, DeclarationKind, Derived, File, FileId, Group, Interner,
    MethodTable, NameTable, Namespace, NamespaceId, Program, TypeDecl, TypeFieldDecl, TypeKind,
    TypeTable, VariantDecl, first_duplicate,
};
use crate::compiler::model::{Type, TypeHead, TypeId, TypeUse};
use crate::compiler::native_registry::Native;
use crate::compiler::{Diagnostic, duplicate_field, duplicate_in_namespace};
use crate::syntax::{Span, TypeDeclaration};

/// Where a file declaration is written, for registration.
pub(in crate::compiler::resolve_names) struct DeclarationSite<'a> {
    pub(in crate::compiler::resolve_names) kind: DeclarationKind,
    pub(in crate::compiler::resolve_names) namespace: NamespaceId,
    pub(in crate::compiler::resolve_names) name: &'a str,
    pub(in crate::compiler::resolve_names) name_span: Span,
    /// Whether another namespace may reach the declaration.
    pub(in crate::compiler::resolve_names) public: bool,
    /// The type head of the block this declaration is a method of, when it sits
    /// in an `impl` block and its first parameter has the block's type. What a
    /// `receiver.name(...)` call matches against.
    pub(in crate::compiler::resolve_names) receiver_head: Option<TypeHead>,
    /// The type head of the block this declaration is an associated function
    /// of, when it sits in an `impl` block and its first parameter does not
    /// have the block's type. What a `Type.name(...)` call matches against.
    pub(in crate::compiler::resolve_names) associated_head: Option<TypeHead>,
    pub(in crate::compiler::resolve_names) file: Option<FileId>,
    pub(in crate::compiler::resolve_names) reported_path: &'a std::path::Path,
}

/// What is known about a declaration before its body is linked.
pub(in crate::compiler::resolve_names) struct Skeleton {
    pub(in crate::compiler::resolve_names) kind: DeclarationKind,
    pub(in crate::compiler::resolve_names) name: String,
    pub(in crate::compiler::resolve_names) name_span: Span,
    /// Whether another namespace may reach the declaration.
    pub(in crate::compiler::resolve_names) public: bool,
    /// The type head of the block this declaration is a method of, when it sits
    /// in an `impl` block and its first parameter has the block's type. Known
    /// at registration so a forward method reference can be dispatched before
    /// its target is linked.
    pub(in crate::compiler::resolve_names) receiver_head: Option<TypeHead>,
    /// The type head of the block this declaration is an associated function
    /// of, when it sits in an `impl` block and its first parameter does not
    /// have the block's type. Known at registration so a forward
    /// `Type.name(...)` reference can be dispatched before its target is
    /// linked.
    pub(in crate::compiler::resolve_names) associated_head: Option<TypeHead>,
    pub(in crate::compiler::resolve_names) file: Option<FileId>,
    pub(in crate::compiler::resolve_names) namespace: NamespaceId,
}

impl Skeleton {
    /// Whether the declaration is filed under a type: a method or an
    /// associated function in an `impl` block, or a native whose row gives it a
    /// head. A filed declaration does not own its name alone; a plain function,
    /// a module value, and a type do.
    pub(in crate::compiler::resolve_names) fn filed(&self) -> bool {
        self.kind.is_bound() && (self.receiver_head.is_some() || self.associated_head.is_some())
    }
}

/// A worklist entry for the linking pass: a declaration whose body comes from
/// one parsed item. Only a callable and a module value own a body, so the two
/// forms are the variants and the linking pass matches one instead of testing a
/// kind and falling back to an unreachable arm.
pub(in crate::compiler::resolve_names) enum Pending {
    Callable {
        declaration: DeclarationId,
        file: FileId,
        /// The parsed declaration in the loaded item stream.
        item: usize,
        namespace: NamespaceId,
    },
    ModuleValue {
        declaration: DeclarationId,
        file: FileId,
        /// The parsed declaration in the loaded item stream.
        item: usize,
        namespace: NamespaceId,
    },
}

impl Pending {
    /// The declaration node the item fills, and the item's place in the loaded
    /// stream.
    pub(in crate::compiler::resolve_names) fn parts(
        &self,
    ) -> (DeclarationId, FileId, usize, NamespaceId) {
        match self {
            Pending::Callable {
                declaration,
                file,
                item,
                namespace,
            }
            | Pending::ModuleValue {
                declaration,
                file,
                item,
                namespace,
            } => (*declaration, *file, *item, *namespace),
        }
    }
}

/// A filled declaration slot: a parameter/local binding (storage), or a native
/// callable (the runtime provides it). A function or module value is left open
/// and filled during linking.
pub(in crate::compiler::resolve_names) enum Slot {
    /// A parameter or local binding: storage for the declared type.
    Binding(Type),
    /// A native callable: its type and its runtime index.
    Native(Type, Native),
}

pub(in crate::compiler::resolve_names) struct Builder {
    pub(in crate::compiler::resolve_names) namespaces: Vec<Namespace>,
    pub(in crate::compiler::resolve_names) skeletons: Vec<Skeleton>,
    /// A node for every declaration, indexed by `DeclarationId`. A callable or
    /// module value starts as a placeholder that the first linking pass fills
    /// before any body is linked, so every id a body can name has its node when
    /// the body is typed. A parameter and a local are created with their type.
    pub(in crate::compiler::resolve_names) declarations: Vec<Declaration>,
    pub(in crate::compiler::resolve_names) files: Vec<File>,
    pub(in crate::compiler::resolve_names) types: Vec<TypeDecl>,
    pub(in crate::compiler::resolve_names) pending: Vec<Pending>,
    /// Every module value, in declaration order. Registration is the one place
    /// that knows them, so ordering their initializers never scans the whole
    /// declaration table.
    pub(in crate::compiler::resolve_names) module_values: Vec<DeclarationId>,
    /// The one interner every name-table key came from. Registration interns
    /// a declaration's, a namespace's, and a type's name exactly once.
    pub(in crate::compiler::resolve_names) intern: Interner,
    /// Which `use` items a resolution actually read, as the loaded stream's
    /// indices. The reads record themselves where they happen, so this is the
    /// one fact the unused-`use` warning reads. The resolution's lookup
    /// methods take `&self`, so the set keeps to the cell until the read
    /// sites collect it.
    pub(in crate::compiler::resolve_names) used_uses: RefCell<HashSet<usize>>,
    /// Every written type name the resolution met, in source order.
    pub(in crate::compiler::resolve_names) type_uses: RefCell<Vec<TypeUse>>,
}

impl Builder {
    pub(in crate::compiler::resolve_names) fn new() -> Self {
        let root = Namespace {
            name: String::new(),
            parent: None,
            children: HashMap::new(),
            names: NameTable::default(),
            methods: MethodTable::default(),
            associates: MethodTable::default(),
            types: TypeTable::default(),
            brought: HashMap::new(),
            filed: HashSet::new(),
        };
        Self {
            namespaces: vec![root],
            skeletons: Vec::new(),
            declarations: Vec::new(),
            files: Vec::new(),
            types: Vec::new(),
            pending: Vec::new(),
            module_values: Vec::new(),
            intern: Interner::default(),
            used_uses: RefCell::new(HashSet::new()),
            type_uses: RefCell::new(Vec::new()),
        }
    }

    pub(in crate::compiler::resolve_names) fn finish(
        self,
        module_values: Vec<DeclarationId>,
    ) -> Program {
        Program {
            files: self.files,
            namespaces: self.namespaces,
            declarations: self.declarations,
            types: self.types,
            module_values,
            intern: self.intern,
            type_uses: self.type_uses.into_inner(),
        }
    }

    /// The namespace node for a path, creating missing segments. Each segment
    /// names its parent's children by the symbol it is interned under, which
    /// is also the key every walk over the tree reads.
    pub(in crate::compiler::resolve_names) fn ensure_namespace(
        &mut self,
        segments: &[String],
    ) -> NamespaceId {
        let mut current = NamespaceId(0);
        for segment in segments {
            let symbol = self.intern.intern(segment);
            current = match self.namespaces[current.0].children.get(&symbol) {
                Some(&child) => child,
                None => {
                    let child = NamespaceId(self.namespaces.len());
                    self.namespaces.push(Namespace {
                        name: segment.clone(),
                        parent: Some(current),
                        children: HashMap::new(),
                        names: NameTable::default(),
                        methods: MethodTable::default(),
                        associates: MethodTable::default(),
                        types: TypeTable::default(),
                        brought: HashMap::new(),
                        filed: HashSet::new(),
                    });
                    self.namespaces[current.0].children.insert(symbol, child);
                    child
                }
            };
        }
        current
    }

    /// Create one node and store it in the model: its skeleton and its
    /// declaration slot. A slot stays open (`cell` is `None`) when the linking
    /// pass will fill the type and value later. Name registration is the
    /// caller's task, because file declarations and local bindings register
    /// differently.
    pub(in crate::compiler::resolve_names) fn create_node(
        &mut self,
        skeleton: Skeleton,
        owner: Option<DeclarationId>,
        cell: Option<Slot>,
    ) -> DeclarationId {
        let id = DeclarationId(self.skeletons.len());
        let kind = skeleton.kind;
        let name = skeleton.name.clone();
        let name_span = skeleton.name_span;
        let file = skeleton.file;
        let namespace = skeleton.namespace;
        let public = skeleton.public;
        self.skeletons.push(skeleton);
        let declaration = match cell {
            Some(Slot::Binding(ty)) => Declaration {
                kind,
                name,
                name_span,
                public,
                file,
                namespace,
                owner,
                ty,
                value: None,
                native: None,
                derived: Derived { halts: false },
            },
            Some(Slot::Native(ty, native)) => Declaration {
                kind,
                name,
                name_span,
                public,
                file,
                namespace,
                owner,
                ty,
                value: None,
                native: Some(native),
                derived: Derived { halts: false },
            },
            // A callable or module value: its phase fills the real type and
            // value. Until then it holds the placeholder type; no body is
            // linked before the first pass settles every one of them.
            None => Declaration {
                kind,
                name,
                name_span,
                public,
                file,
                namespace,
                owner,
                ty: Type::Text,
                value: None,
                native: None,
                derived: Derived { halts: false },
            },
        };
        self.declarations.push(declaration);
        if kind.module_value() {
            self.module_values.push(id);
        }
        id
    }

    /// Reserve a declaration node and register it in its namespace.
    ///
    /// A declaration in an `impl` block is filed under the block's type: a
    /// method in the method pool under `(head, name)` and an associated
    /// function in the associated-function pool under `(head, name)`. A filed
    /// declaration is not a namespace-level name, so it is not appended to the
    /// name pool: it is reached only through its type. Every other declaration
    /// is appended to the name pool, where a name holds exactly one. A filed
    /// declaration is a duplicate when a declaration that owns its name alone
    /// already exists; a name-owning declaration is a duplicate when any
    /// declaration of the name already exists; and two filed declarations
    /// collide only when they share a pool and a head.
    pub(in crate::compiler::resolve_names) fn declare(
        &mut self,
        site: DeclarationSite<'_>,
    ) -> Result<DeclarationId, Diagnostic> {
        let DeclarationSite {
            kind,
            namespace,
            name,
            name_span,
            public,
            receiver_head,
            associated_head,
            file,
            reported_path,
        } = site;
        let id = self.create_node(
            Skeleton {
                kind,
                name: name.to_owned(),
                name_span,
                public,
                receiver_head,
                associated_head,
                file,
                namespace,
            },
            None,
            None,
        );
        let filed = self.skeletons[id.0].filed();
        let symbol = self.intern.intern(name);
        // A name-owning declaration may repeat a name only against nothing; a
        // filed declaration collides with it by name alone, because the filed
        // declaration is reached through its type, never by the name.
        let owning = self.namespaces[namespace.0].names.get(symbol);
        let duplicate = if let Some(existing) = owning {
            Err(existing)
        } else {
            let table = &mut self.namespaces[namespace.0];
            match receiver_head {
                Some(head) => table.methods.insert(head, symbol, id),
                None => match associated_head {
                    Some(head) => table.associates.insert(head, symbol, id),
                    None if !filed && table.filed.contains(&symbol) => Err(id),
                    None => Ok(()),
                },
            }
        };
        if let Err(existing) = duplicate {
            let mut diagnostic =
                Diagnostic::new(reported_path, name_span, duplicate_in_namespace(name));
            // The filed pool knows a name was taken, not by which node, so the
            // first place is attached only when it is known.
            if existing != id
                && let Some(related_file) = self.skeletons[existing.0].file
            {
                diagnostic = diagnostic.with_related(
                    self.files[related_file.0].path.clone(),
                    self.skeletons[existing.0].name_span,
                    crate::compiler::diagnostic::first_declaration(name),
                );
            }
            return Err(diagnostic);
        }
        let table = &mut self.namespaces[namespace.0];
        if filed {
            table.filed.insert(symbol);
        } else {
            table.names.insert(symbol, id);
        }
        Ok(id)
    }

    /// Whether `caller` may reach a bound function filed in `owner`: in its own
    /// namespace always, and across namespaces only when the function is `pub`
    /// and its type is `pub`. A bound function is never gated by a `use`. The
    /// one rule is the model's; the skeletons supply its facts.
    pub(in crate::compiler::resolve_names) fn bound_reachable(
        &self,
        caller: NamespaceId,
        owner: NamespaceId,
        declaration: DeclarationId,
    ) -> bool {
        let skeleton = &self.skeletons[declaration.0];
        let type_public = match skeleton.receiver_head.or(skeleton.associated_head) {
            Some(TypeHead::Named(id)) => Some(self.types[id.0].public),
            _ => None,
        };
        crate::compiler::model::bound_reachable(skeleton.public, type_public, owner, caller)
    }

    /// Reject a repeated type parameter, whichever declaration carries it: a
    /// `rec`, an `enum`, or a function. Both the declared-type path and the
    /// function path check through this one scan.
    pub(in crate::compiler::resolve_names) fn check_type_parameters(
        &self,
        parameters: &[String],
        name_span: Span,
        reported_path: &std::path::Path,
    ) -> Result<(), Diagnostic> {
        if let Some(index) = first_duplicate(parameters.iter().map(String::as_str)) {
            return Err(Diagnostic::new(
                reported_path,
                name_span,
                format!("duplicate type parameter `{}`", parameters[index]),
            ));
        }
        Ok(())
    }

    /// Reserve a declared type: check its parameters, allocate its id, and
    /// register its name, before any field type is resolved. Every declared type
    /// is reserved before any is filled, so a type may name itself and any other
    /// type, whatever the declaration order.
    pub(in crate::compiler::resolve_names) fn reserve_type(
        &mut self,
        declaration: &TypeDeclaration,
        public: bool,
        namespace: NamespaceId,
        file: FileId,
        reported_path: &std::path::Path,
    ) -> Result<TypeId, Diagnostic> {
        self.check_type_parameters(
            &declaration.parameters,
            declaration.name_span,
            reported_path,
        )?;
        // A `rec` and an `enum` are filled by `fill_type`; the placeholder
        // keeps every reserved id valid while the remaining types are reserved.
        let kind = TypeKind::Rec(Vec::new());
        let id = TypeId(self.types.len());
        self.types.push(TypeDecl {
            name: declaration.name.clone(),
            name_span: declaration.name_span,
            file: Some(file),
            public,
            namespace,
            parameters: declaration.parameters.clone(),
            runtime: crate::compiler::model::RuntimeType::Declared,
            kind,
        });
        if self.namespaces[namespace.0]
            .types
            .insert(self.intern.intern(&declaration.name), id)
            .is_err()
        {
            return Err(Diagnostic::new(
                reported_path,
                declaration.name_span,
                duplicate_in_namespace(&declaration.name),
            ));
        }
        Ok(id)
    }

    /// Fill a reserved declared type: resolve its field or variant types and
    /// record them.
    pub(in crate::compiler::resolve_names) fn fill_type(
        &mut self,
        file: FileId,
        declaration: &TypeDeclaration,
        id: TypeId,
        group: Group,
        namespace: NamespaceId,
        reported_path: &std::path::Path,
    ) -> Result<(), Diagnostic> {
        let parameters = declaration.parameters.clone();
        let kind = if group.has_fields() {
            let fields = declaration
                .fields()
                .expect("a group with fields carries a rec body");
            if let Some(index) = first_duplicate(fields.iter().map(|field| field.name.as_str())) {
                let field = &fields[index];
                return Err(Diagnostic::new(
                    reported_path,
                    field.name_span,
                    duplicate_field(&field.name),
                ));
            }
            let mut declared = Vec::with_capacity(fields.len());
            for field in fields {
                let ty =
                    self.resolve_type_expr(&field.ty, namespace, file, &parameters, reported_path)?;
                declared.push(TypeFieldDecl {
                    name: field.name.clone(),
                    name_span: field.name_span,
                    public: field.public,
                    ty,
                });
            }
            TypeKind::Rec(declared)
        } else {
            let variants = declaration
                .variants()
                .expect("a group with variants carries an enum body");
            if let Some(index) =
                first_duplicate(variants.iter().map(|variant| variant.name.as_str()))
            {
                let variant = &variants[index];
                return Err(Diagnostic::new(
                    reported_path,
                    variant.name_span,
                    format!("duplicate variant `{}`", variant.name),
                ));
            }
            let mut declared = Vec::with_capacity(variants.len());
            for variant in variants {
                let mut payload = Vec::with_capacity(variant.payload.len());
                for ty in &variant.payload {
                    payload.push(self.resolve_type_expr(
                        ty,
                        namespace,
                        file,
                        &parameters,
                        reported_path,
                    )?);
                }
                declared.push(VariantDecl {
                    name: variant.name.clone(),
                    name_span: variant.name_span,
                    public: variant.public,
                    payload,
                });
            }
            TypeKind::Enum(declared)
        };
        self.types[id.0].kind = kind;
        Ok(())
    }
}
