//! Registering declaration nodes: reserving types, declaring names, and
//! filling a type's fields.

use std::collections::HashMap;

use crate::compiler::model::{
    Declaration, DeclarationId, DeclarationKind, Derived, File, FileId, Group, MethodTable,
    NameTable, Namespace, NamespaceId, Program, TypeDecl, TypeFieldDecl, TypeKind, TypeTable,
    VariantDecl, first_duplicate, reachable,
};
use crate::compiler::model::{Type, TypeHead, TypeId};
use crate::compiler::{Diagnostic, duplicate_field, duplicate_in_namespace};
use crate::native_registry::Native;
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
    pub(in crate::compiler::resolve_names) syntax_index: Option<usize>,
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
    /// The index of the parsed declaration in its file, when it came from one.
    pub(in crate::compiler::resolve_names) syntax_index: Option<usize>,
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

/// A worklist entry for the linking pass.
pub(in crate::compiler::resolve_names) struct Pending {
    pub(in crate::compiler::resolve_names) declaration: DeclarationId,
    pub(in crate::compiler::resolve_names) file: FileId,
    pub(in crate::compiler::resolve_names) syntax_index: usize,
    pub(in crate::compiler::resolve_names) namespace: NamespaceId,
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
    pub(in crate::compiler::resolve_names) declarations: Vec<Option<Declaration>>,
    pub(in crate::compiler::resolve_names) files: Vec<File>,
    pub(in crate::compiler::resolve_names) types: Vec<TypeDecl>,
    pub(in crate::compiler::resolve_names) pending: Vec<Pending>,
    /// The declared absence type: the `std` category's `Option`, found while
    /// reserving types. It types a map read and a missing value.
    pub(in crate::compiler::resolve_names) absence_type: Option<TypeId>,
    /// The runtime's `Vec` type, registered into `std::dynamic` before the
    /// natives. The lowering reads it to lower the vec operations.
    pub(in crate::compiler::resolve_names) vec_type: Option<TypeId>,
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
            uses: Vec::new(),
        };
        Self {
            namespaces: vec![root],
            skeletons: Vec::new(),
            declarations: Vec::new(),
            files: Vec::new(),
            types: Vec::new(),
            pending: Vec::new(),
            absence_type: None,
            vec_type: None,
        }
    }

    pub(in crate::compiler::resolve_names) fn finish(
        self,
        entry: DeclarationId,
        module_values: Vec<DeclarationId>,
    ) -> Program {
        Program {
            files: self.files,
            namespaces: self.namespaces,
            declarations: self
                .declarations
                .into_iter()
                .map(|declaration| declaration.expect("every node is built during linking"))
                .collect(),
            types: self.types,
            absence_type: self.absence_type,
            vec_type: self.vec_type,
            entry,
            module_values,
        }
    }

    /// The namespace node for a path, creating missing segments.
    pub(in crate::compiler::resolve_names) fn ensure_namespace(
        &mut self,
        segments: &[String],
    ) -> NamespaceId {
        let mut current = NamespaceId(0);
        for segment in segments {
            current = match self.namespaces[current.0].children.get(segment) {
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
                        uses: Vec::new(),
                    });
                    self.namespaces[current.0]
                        .children
                        .insert(segment.clone(), child);
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
        let file = skeleton.file;
        let namespace = skeleton.namespace;
        self.skeletons.push(skeleton);
        self.declarations.push(cell.map(|slot| {
            let (ty, native) = match slot {
                Slot::Binding(ty) => (ty, None),
                Slot::Native(ty, native) => (ty, Some(native)),
            };
            Declaration {
                kind,
                name,
                file,
                namespace,
                owner,
                ty,
                value: None,
                native,
                derived: Derived { halts: false },
            }
        }));
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
            syntax_index,
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
                syntax_index,
            },
            None,
            None,
        );
        let filed = self.skeletons[id.0].filed();
        // A name-owning declaration may repeat a name only against nothing; a
        // filed declaration collides with it by name alone, because the filed
        // declaration is reached through its type, never by the name.
        let owning = self.namespaces[namespace.0].names.contains(name);
        let filed_name_taken = !filed && self.filed_named(namespace, name).is_some();
        let duplicate = {
            let table = &mut self.namespaces[namespace.0];
            if let Some(head) = receiver_head {
                owning || table.methods.insert(head, name.to_owned(), id).is_err()
            } else if let Some(head) = associated_head {
                owning || table.associates.insert(head, name.to_owned(), id).is_err()
            } else {
                owning || filed_name_taken
            }
        };
        if duplicate {
            return Err(Diagnostic::new(
                reported_path,
                name_span,
                duplicate_in_namespace(name),
            ));
        }
        if !filed {
            self.namespaces[namespace.0]
                .names
                .insert(name.to_owned(), id);
        }
        Ok(id)
    }

    /// A function filed under a type in `namespace`, whichever head. A filed
    /// function is not a namespace-level name, so a bare name never reaches it
    /// and a `use` of it is rejected.
    pub(in crate::compiler::resolve_names) fn filed_named(
        &self,
        namespace: NamespaceId,
        name: &str,
    ) -> Option<DeclarationId> {
        let table = &self.namespaces[namespace.0];
        table
            .methods
            .iter()
            .chain(table.associates.iter())
            .copied()
            .find(|declaration| self.skeletons[declaration.0].name == name)
    }

    /// The name of the type a filed function belongs to, for a diagnostic.
    pub(in crate::compiler::resolve_names) fn bound_type_name(
        &self,
        declaration: DeclarationId,
    ) -> String {
        let skeleton = &self.skeletons[declaration.0];
        match skeleton.receiver_head.or(skeleton.associated_head) {
            Some(TypeHead::Named(id)) => self.types[id.0].name.clone(),
            Some(TypeHead::Scalar(name)) => name.to_owned(),
            _ => unreachable!("a filed function belongs to a named or scalar type"),
        }
    }

    /// Whether `caller` may reach a bound function filed in `owner`: in its own
    /// namespace always, and across namespaces only when the function is `pub`
    /// and its type is `pub`. A bound function is never gated by a `use`.
    pub(in crate::compiler::resolve_names) fn bound_reachable(
        &self,
        caller: NamespaceId,
        owner: NamespaceId,
        declaration: DeclarationId,
    ) -> bool {
        let skeleton = &self.skeletons[declaration.0];
        if !reachable(skeleton.public, owner, caller) {
            return false;
        }
        match skeleton.receiver_head.or(skeleton.associated_head) {
            Some(TypeHead::Named(id)) => reachable(self.types[id.0].public, owner, caller),
            _ => true,
        }
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
        group: Group,
        namespace: NamespaceId,
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
        // The declared absence type is the `std` category's `Option`: the one
        // enum whose payloadless variant marks a missing value. A program may
        // declare its own `Option`, but only the standard library's is the
        // absence.
        if declaration.name == "Option"
            && group == Group::Enum
            && self.namespaces[namespace.0].name == "std"
        {
            self.absence_type = Some(id);
        }
        self.types.push(TypeDecl {
            name: declaration.name.clone(),
            public,
            namespace,
            parameters: declaration.parameters.clone(),
            kind,
        });
        if self.namespaces[namespace.0]
            .types
            .insert(declaration.name.clone(), id)
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
                    self.resolve_type_expr(&field.ty, namespace, &parameters, reported_path)?;
                declared.push(TypeFieldDecl {
                    name: field.name.clone(),
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
                        &parameters,
                        reported_path,
                    )?);
                }
                declared.push(VariantDecl {
                    name: variant.name.clone(),
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
