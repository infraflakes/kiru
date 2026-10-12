//! The builder: reserve declaration nodes and register their names.
//!
//! Registration builds the skeleton of the model: namespaces, declaration
//! nodes, and the worklist of bodies still to link. The linking pass fills
//! every body. A namespace keeps a name pool of every declaration under a name
//! and a method pool keyed by receiver head and name, so a method name may
//! repeat once per head.

use std::collections::HashMap;

use crate::compiler::{Diagnostic, duplicate_field, duplicate_in_namespace};
use crate::model::{BuiltinType, Type, TypeHead, TypeId};
use crate::model::{
    Declaration, DeclarationId, Derived, File, FileId, MethodTable, NameTable, Namespace,
    NamespaceId, Program, TypeDecl, TypeFieldDecl, TypeKind, TypeTable, VariantDecl,
    first_duplicate, namespace_at,
};
use crate::native_registry::Native;
use crate::syntax::{Span, TypeDeclaration, TypeDeclarationKind, TypeExpr};

/// Where a file declaration is written, for registration.
pub(in crate::compiler::resolve_names) struct DeclarationSite<'a> {
    pub(in crate::compiler::resolve_names) namespace: NamespaceId,
    pub(in crate::compiler::resolve_names) name: &'a str,
    pub(in crate::compiler::resolve_names) name_span: Span,
    /// Whether another namespace may reach the declaration.
    pub(in crate::compiler::resolve_names) public: bool,
    /// The type head of the declaration's first parameter, when it is a
    /// callable: what a `receiver.name(...)` call matches against.
    pub(in crate::compiler::resolve_names) first_parameter_head: Option<TypeHead>,
    pub(in crate::compiler::resolve_names) file: Option<FileId>,
    pub(in crate::compiler::resolve_names) syntax_index: Option<usize>,
    pub(in crate::compiler::resolve_names) reported_path: &'a std::path::Path,
}

/// What is known about a declaration before its body is linked.
pub(in crate::compiler::resolve_names) struct Skeleton {
    pub(in crate::compiler::resolve_names) name: String,
    pub(in crate::compiler::resolve_names) name_span: Span,
    /// Whether another namespace may reach the declaration.
    pub(in crate::compiler::resolve_names) public: bool,
    /// The type head of the declaration's first parameter, when it is a
    /// callable. Known at registration so a forward method reference can be
    /// dispatched (and rejected for order) before its target is linked.
    pub(in crate::compiler::resolve_names) first_parameter_head: Option<TypeHead>,
    pub(in crate::compiler::resolve_names) file: Option<FileId>,
    pub(in crate::compiler::resolve_names) namespace: NamespaceId,
    pub(in crate::compiler::resolve_names) order: usize,
    /// The index of the parsed declaration in its file, when it came from one.
    pub(in crate::compiler::resolve_names) syntax_index: Option<usize>,
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
    pub(in crate::compiler::resolve_names) order: usize,
    /// The declared absence type: the `std` category's `Option`, found while
    /// reserving types. It types a map read and a missing value.
    pub(in crate::compiler::resolve_names) absence_type: Option<TypeId>,
}

impl Builder {
    pub(in crate::compiler::resolve_names) fn new() -> Self {
        let root = Namespace {
            name: String::new(),
            parent: None,
            children: HashMap::new(),
            names: NameTable::default(),
            methods: MethodTable::default(),
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
            order: 0,
            absence_type: None,
        }
    }

    pub(in crate::compiler::resolve_names) fn finish(self, entry: DeclarationId) -> Program {
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
            entry,
        }
    }

    /// The namespace node for a path, creating missing segments.
    pub(super) fn ensure_namespace(&mut self, segments: &[String]) -> NamespaceId {
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
        let name = skeleton.name.clone();
        let file = skeleton.file;
        let namespace = skeleton.namespace;
        let order = skeleton.order;
        self.skeletons.push(skeleton);
        self.declarations.push(cell.map(|slot| {
            let (ty, native) = match slot {
                Slot::Binding(ty) => (ty, None),
                Slot::Native(ty, native) => (ty, Some(native)),
            };
            Declaration {
                name,
                file,
                namespace,
                order,
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
    /// A declaration whose first parameter has a type head is a method: it is
    /// registered in the method pool under `(head, name)` and appended to the
    /// name pool. A declaration with no head is appended to the name pool
    /// alone. A name may repeat only as methods that differ by their receiver's
    /// type head: a method is a duplicate when a head-less declaration of the
    /// name already exists, a head-less declaration is a duplicate when any
    /// declaration of the name already exists, and two methods collide only
    /// when their head matches.
    pub(super) fn declare(
        &mut self,
        site: DeclarationSite<'_>,
    ) -> Result<DeclarationId, Diagnostic> {
        let DeclarationSite {
            namespace,
            name,
            name_span,
            public,
            first_parameter_head,
            file,
            syntax_index,
            reported_path,
        } = site;
        let order = self.order;
        let id = self.create_node(
            Skeleton {
                name: name.to_owned(),
                name_span,
                public,
                first_parameter_head,
                file,
                namespace,
                order,
                syntax_index,
            },
            None,
            None,
        );
        // A head-less declaration owns the whole name, so a method may repeat a
        // name only against other methods. The name pool holds every
        // declaration, so a method conflicts exactly when the name already
        // holds a head-less declaration; the method pool then decides the
        // head-collision case.
        let headless = self.namespaces[namespace.0]
            .names
            .get(name)
            .iter()
            .any(|existing| self.skeletons[existing.0].first_parameter_head.is_none());
        let table = &mut self.namespaces[namespace.0];
        let duplicate = match first_parameter_head {
            Some(head) => headless || table.methods.insert(head, name.to_owned(), id).is_err(),
            None => table.names.contains(name),
        };
        if duplicate {
            return Err(Diagnostic::new(
                reported_path,
                name_span,
                duplicate_in_namespace(name),
            ));
        }
        table.names.insert(name.to_owned(), id);
        self.order += 1;
        Ok(id)
    }

    /// Reject a repeated type parameter, whichever declaration carries it: a
    /// `rec`, an `enum`, or a function. Both the declared-type path and the
    /// function path check through this one scan.
    pub(super) fn check_type_parameters(
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
    pub(super) fn reserve_type(
        &mut self,
        declaration: &TypeDeclaration,
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
            && matches!(declaration.kind, TypeDeclarationKind::Enum(_))
            && self.namespaces[namespace.0].name == "std"
        {
            self.absence_type = Some(id);
        }
        self.types.push(TypeDecl {
            name: declaration.name.clone(),
            public: declaration.public,
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
    pub(super) fn fill_type(
        &mut self,
        declaration: &TypeDeclaration,
        id: TypeId,
        namespace: NamespaceId,
        reported_path: &std::path::Path,
    ) -> Result<(), Diagnostic> {
        let parameters = declaration.parameters.clone();
        let kind = match &declaration.kind {
            TypeDeclarationKind::Rec(fields) => {
                if let Some(index) = first_duplicate(fields.iter().map(|field| field.name.as_str()))
                {
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
            }
            TypeDeclarationKind::Enum(variants) => {
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
            }
        };
        self.types[id.0].kind = kind;
        self.order += 1;
        Ok(())
    }

    /// Resolve a written type to a `Type`.
    ///
    /// Every written type resolves the same way: the name finds the
    /// declaration it names, and the declaration decides the result — a
    /// built-in marker yields the runtime representation the marker carries,
    /// and a program `rec`/`enum` yields `Type::Named`. A written name
    /// (`txt`, `Vec<T>`, a bare name) differs only in how it
    /// spells the name and arguments.
    pub(in crate::compiler::resolve_names) fn resolve_type_expr(
        &self,
        expr: &TypeExpr,
        namespace: NamespaceId,
        parameters: &[String],
        reported_path: &std::path::Path,
    ) -> Result<Type, Diagnostic> {
        match expr {
            TypeExpr::Function {
                parameters: declared,
                returns,
                ..
            } => {
                let mut resolved = Vec::with_capacity(declared.len());
                for parameter in declared {
                    resolved.push(self.resolve_type_expr(
                        parameter,
                        namespace,
                        parameters,
                        reported_path,
                    )?);
                }
                // No arrow means the function returns no value: its function
                // type has no return type, and there is no type that names it.
                let returns = match returns {
                    Some(returns) => Some(Box::new(self.resolve_type_expr(
                        returns,
                        namespace,
                        parameters,
                        reported_path,
                    )?)),
                    None => None,
                };
                Ok(Type::Function {
                    parameters: resolved,
                    returns,
                })
            }
            TypeExpr::Named {
                root,
                path,
                arguments,
                span,
            } => {
                if !root
                    && path.len() == 1
                    && arguments.is_empty()
                    && parameters.iter().any(|parameter| parameter == &path[0])
                {
                    return Ok(Type::Parameter(path[0].clone()));
                }
                self.resolve_named_type(
                    WrittenType {
                        path,
                        root: *root,
                        span,
                    },
                    arguments,
                    namespace,
                    parameters,
                    reported_path,
                )
            }
        }
    }

    /// Resolve a written type by the declaration it names: find the
    /// declaration, check its type-argument count, resolve the arguments, and
    /// read its kind for the result.
    fn resolve_named_type(
        &self,
        written: WrittenType<'_>,
        arguments: &[TypeExpr],
        namespace: NamespaceId,
        parameters: &[String],
        reported_path: &std::path::Path,
    ) -> Result<Type, Diagnostic> {
        let WrittenType { path, root, span } = written;
        // The intrinsics are the language's own types, not declarations: a
        // bare, non-rooted, single-segment name names one directly, before any
        // declaration is looked up. Nothing can delete `txt` or `Vec` by
        // deleting a declaration, because there is none.
        if !root
            && path.len() == 1
            && let Some(builtin) = BuiltinType::from_name(&path[0])
        {
            let parameter_count = builtin.parameter_count();
            if parameter_count != arguments.len() {
                return Err(Diagnostic::new(
                    reported_path,
                    *span,
                    format!(
                        "`{}` takes {}, found {}",
                        path[0],
                        crate::compiler::diagnostics::count(parameter_count, "type argument"),
                        arguments.len()
                    ),
                ));
            }
            let mut resolved = Vec::with_capacity(arguments.len());
            for argument in arguments {
                resolved.push(self.resolve_type_expr(
                    argument,
                    namespace,
                    parameters,
                    reported_path,
                )?);
            }
            return Ok(builtin.runtime_type(&resolved));
        }
        let Some(id) = self.find_type(namespace, path, root) else {
            return Err(Diagnostic::new(
                reported_path,
                *span,
                format!("unknown type `{}`", crate::model::join_path(path, root)),
            ));
        };
        let declared = &self.types[id.0];
        let name = declared.name.clone();
        let parameter_count = declared.parameters.len();
        if parameter_count != arguments.len() {
            return Err(Diagnostic::new(
                reported_path,
                *span,
                format!(
                    "`{name}` takes {}, found {}",
                    crate::compiler::diagnostics::count(parameter_count, "type argument"),
                    arguments.len()
                ),
            ));
        }
        let mut resolved = Vec::with_capacity(arguments.len());
        for argument in arguments {
            resolved.push(self.resolve_type_expr(
                argument,
                namespace,
                parameters,
                reported_path,
            )?);
        }
        Ok(Type::Named(id, resolved))
    }

    /// Resolve a `Type::Variant` path to the enum's type and the variant's
    /// index, when the path names a declared enum variant. A path with one
    /// segment is never a variant.
    pub(in crate::compiler::resolve_names) fn resolve_variant(
        &self,
        namespace: NamespaceId,
        path: &[String],
        root: bool,
    ) -> Option<(TypeId, usize)> {
        let (variant, type_segments) = path.split_last()?;
        if type_segments.is_empty() {
            return None;
        }
        let id = self.find_type(namespace, type_segments, root)?;
        let TypeKind::Enum(variants) = &self.types[id.0].kind else {
            return None;
        };
        let index = variants
            .iter()
            .position(|declared| &declared.name == variant)?;
        Some((id, index))
    }

    /// Look up a name through one `use` path: the path names either a
    /// namespace, whose entries become visible, or one declaration, whose name
    /// must match. Only a public entry is reachable. A type and a value resolve
    /// through this one walk, each supplying its own table. The entry lookup
    /// returns every declaration under the name, so a method family is reached
    /// whole.
    pub(in crate::compiler::resolve_names) fn used_lookup<T: Copy>(
        &self,
        used: &[String],
        name: &str,
        entry: impl Fn(NamespaceId, &str) -> Vec<T>,
        public: impl Fn(T) -> bool,
    ) -> Vec<T> {
        if let Some(namespace) = namespace_at(&self.namespaces, used) {
            return entry(namespace, name)
                .into_iter()
                .filter(|found| public(*found))
                .collect();
        }
        // A longer path names one declaration, which may be the entry.
        let (used_name, segments) = used
            .split_last()
            .expect("a use path has at least one segment");
        if used_name != name {
            return Vec::new();
        }
        let Some(namespace) = namespace_at(&self.namespaces, segments) else {
            return Vec::new();
        };
        entry(namespace, used_name)
            .into_iter()
            .filter(|found| public(*found))
            .collect()
    }

    /// Find a declared type by a path: a bare single segment reads the current
    /// namespace and the namespaces it `use`s, and anything else starts at the
    /// namespace the path names. Nothing is visible by climbing to a parent.
    pub(in crate::compiler::resolve_names) fn find_type(
        &self,
        namespace: NamespaceId,
        path: &[String],
        root: bool,
    ) -> Option<TypeId> {
        let (name, namespace_segments) = path
            .split_last()
            .expect("a type path has at least one segment");
        let current = namespace;
        if !root && namespace_segments.is_empty() {
            // The current namespace, then each namespace this one `use`s.
            if let Some(found) = self.namespaces[current.0].type_id(name) {
                return Some(found);
            }
            for used in &self.namespaces[current.0].uses {
                if let Some(found) = self
                    .used_lookup(
                        used,
                        name,
                        |namespace, name| {
                            self.namespaces[namespace.0]
                                .type_id(name)
                                .into_iter()
                                .collect()
                        },
                        |found| self.types[found.0].public,
                    )
                    .into_iter()
                    .next()
                {
                    return Some(found);
                }
            }
            // The intrinsics are the language's own types and are resolved
            // before `find_type` is reached, so a bare name here is only a
            // declaration. Every other bare name resolves through this
            // namespace's own types and the namespaces it `use`s.
            return None;
        }
        let namespace = if namespace_segments.is_empty() {
            NamespaceId(0)
        } else {
            namespace_at(&self.namespaces, namespace_segments)?
        };
        let found = self.namespaces[namespace.0].type_id(name)?;
        (namespace == current || self.types[found.0].public).then_some(found)
    }
}

/// A written type name: its path, whether it is rooted, and its span. Every
/// written name resolves through one helper, which takes this.
struct WrittenType<'a> {
    path: &'a [String],
    root: bool,
    span: &'a Span,
}
