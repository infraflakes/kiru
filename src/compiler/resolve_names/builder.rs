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
    Declaration, DeclarationId, Derived, Expression, File, FileId, MethodTable, NameTable,
    Namespace, NamespaceId, Program, StringPart, TypeDecl, TypeFieldDecl, TypeKind, TypeTable,
    VariantDecl, first_duplicate, namespace_at,
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
    /// The type head of the declaration's result, when it is a callable with
    /// no parameters: what a `Type.name(...)` constructor call matches against.
    pub(in crate::compiler::resolve_names) constructor_head: Option<TypeHead>,
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
    /// dispatched before its target is linked.
    pub(in crate::compiler::resolve_names) first_parameter_head: Option<TypeHead>,
    /// The type head of the declaration's result, when it is a callable with
    /// no parameters. Known at registration so a forward constructor
    /// reference can be dispatched before its target is linked.
    pub(in crate::compiler::resolve_names) constructor_head: Option<TypeHead>,
    pub(in crate::compiler::resolve_names) file: Option<FileId>,
    pub(in crate::compiler::resolve_names) namespace: NamespaceId,
    /// The index of the parsed declaration in its file, when it came from one.
    pub(in crate::compiler::resolve_names) syntax_index: Option<usize>,
}

impl Skeleton {
    /// The head the method pool keys this declaration under: a method's
    /// receiver head, or a constructor's result head. `None` for a declaration
    /// with neither, which owns its name alone.
    pub(in crate::compiler::resolve_names) fn method_key(&self) -> Option<TypeHead> {
        self.first_parameter_head.or(self.constructor_head)
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

    /// The module values in dependency order: a value's initializer is
    /// evaluated before the values that read it. Only a reference in the
    /// initializer itself counts, and only another module value in the same
    /// namespace is a dependency — a module value is never `pub`, so no value
    /// crosses a namespace. A cycle among them is an error.
    pub(in crate::compiler::resolve_names) fn order_module_values(
        &self,
    ) -> Result<Vec<DeclarationId>, Diagnostic> {
        let count = self.declarations.len();
        let mut deps: Vec<Vec<DeclarationId>> = vec![Vec::new(); count];
        for (index, dependencies) in deps.iter_mut().enumerate() {
            let id = DeclarationId(index);
            if !self.is_module_value(id) {
                continue;
            }
            let declaration = self.declarations[index]
                .as_ref()
                .expect("every node is built during linking");
            let namespace = declaration.namespace;
            let mut references = Vec::new();
            collect_initializer_references(
                declaration
                    .value
                    .as_ref()
                    .expect("a module value has an initializer"),
                &mut references,
            );
            for reference in references {
                if self.is_module_value(reference)
                    && self.declarations[reference.0]
                        .as_ref()
                        .expect("every node is built during linking")
                        .namespace
                        == namespace
                    && !dependencies.contains(&reference)
                {
                    dependencies.push(reference);
                }
            }
        }
        let mut order = Vec::new();
        // 0 unvisited, 1 on the current path, 2 finished.
        let mut state = vec![0u8; count];
        for index in 0..count {
            let id = DeclarationId(index);
            if self.is_module_value(id) {
                self.visit_module_value(id, &deps, &mut state, &mut order)?;
            }
        }
        Ok(order)
    }

    /// Whether a declaration is a module value: a top-level value that is not a
    /// function. A function is code, not data, so it is evaluated when called.
    fn is_module_value(&self, id: DeclarationId) -> bool {
        self.declarations[id.0].as_ref().is_some_and(|declaration| {
            declaration.value.is_some()
                && declaration.function().is_none()
                && declaration.owner.is_none()
        })
    }

    /// Depth-first visit of one module value, emitting it after its
    /// dependencies. A value reached while already on the path closes a cycle.
    fn visit_module_value(
        &self,
        id: DeclarationId,
        deps: &[Vec<DeclarationId>],
        state: &mut [u8],
        order: &mut Vec<DeclarationId>,
    ) -> Result<(), Diagnostic> {
        match state[id.0] {
            2 => return Ok(()),
            1 => {
                let declaration = self.declarations[id.0]
                    .as_ref()
                    .expect("every node is built during linking");
                let file = declaration
                    .file
                    .expect("a module value is a file declaration, so it has a file");
                return Err(Diagnostic::new(
                    &self.files[file.0].path,
                    self.skeletons[id.0].name_span,
                    format!(
                        "`{}` is part of a cycle among module values",
                        declaration.name
                    ),
                ));
            }
            _ => {}
        }
        state[id.0] = 1;
        for dependency in &deps[id.0] {
            self.visit_module_value(*dependency, deps, state, order)?;
        }
        state[id.0] = 2;
        order.push(id);
        Ok(())
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
    /// name pool. A declaration with no parameters whose result has a head is
    /// a constructor: it is registered under `(result head, name)` the same
    /// way. A declaration with neither head is appended to the name pool alone.
    /// A name may repeat only as keyed declarations that differ by their head:
    /// a keyed declaration is a duplicate when a head-less declaration of the
    /// name already exists, a head-less declaration is a duplicate when any
    /// declaration of the name already exists, and two keyed declarations
    /// collide only when their head matches.
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
            constructor_head,
            file,
            syntax_index,
            reported_path,
        } = site;
        let id = self.create_node(
            Skeleton {
                name: name.to_owned(),
                name_span,
                public,
                first_parameter_head,
                constructor_head,
                file,
                namespace,
                syntax_index,
            },
            None,
            None,
        );
        // A head-less declaration owns the whole name, so a keyed declaration
        // may repeat a name only against other keyed declarations. The name
        // pool holds every declaration, so a keyed declaration conflicts
        // exactly when the name already holds a head-less declaration; the
        // method pool then decides the head-collision case.
        let headless = self.namespaces[namespace.0]
            .names
            .get(name)
            .iter()
            .any(|existing| self.skeletons[existing.0].method_key().is_none());
        let table = &mut self.namespaces[namespace.0];
        let duplicate = match first_parameter_head.or(constructor_head) {
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
                path,
                arguments,
                span,
            } => {
                if path.len() == 1
                    && arguments.is_empty()
                    && parameters.iter().any(|parameter| parameter == &path[0])
                {
                    return Ok(Type::Parameter(path[0].clone()));
                }
                self.resolve_named_type(
                    WrittenType { path, span },
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
        let WrittenType { path, span } = written;
        // The scalar intrinsics are the language's own types, not
        // declarations: a bare single-segment name names one directly, before
        // any declaration is looked up. Nothing can delete `txt` by deleting a
        // declaration, because there is none. `Vec` is not here: it is an
        // ordinary declaration the compiler registers in `std::dynamic`.
        if path.len() == 1
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
            return Ok(builtin.runtime_type());
        }
        let Some(id) = self.find_type(namespace, path) else {
            return Err(Diagnostic::new(
                reported_path,
                *span,
                format!("unknown type `{}`", crate::model::join_path(path)),
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
    ) -> Option<(TypeId, usize)> {
        let (variant, type_segments) = path.split_last()?;
        if type_segments.is_empty() {
            return None;
        }
        let id = self.find_type(namespace, type_segments)?;
        let TypeKind::Enum(variants) = &self.types[id.0].kind else {
            return None;
        };
        let index = variants
            .iter()
            .position(|declared| &declared.name == variant)?;
        Some((id, index))
    }

    /// The namespace a qualified path's first segment names, through the
    /// namespaces this one `use`s: the `use` whose last segment is `first` and
    /// whose whole path is a namespace. A qualified name reaches only the
    /// namespaces in scope; nothing climbs to a parent and nothing starts at
    /// the root.
    pub(in crate::compiler::resolve_names) fn used_namespace(
        &self,
        namespace: NamespaceId,
        first: &str,
    ) -> Option<NamespaceId> {
        for used in &self.namespaces[namespace.0].uses {
            if used.last().map(String::as_str) == Some(first)
                && let Some(base) = namespace_at(&self.namespaces, used)
            {
                return Some(base);
            }
        }
        None
    }

    /// The namespace a name-`use` reaches: the path's last segment is `name`
    /// and the rest names a namespace. `None` when the use names a namespace
    /// (its whole path resolves) or when the path reaches no name.
    pub(in crate::compiler::resolve_names) fn name_use(
        &self,
        used: &[String],
        name: &str,
    ) -> Option<NamespaceId> {
        let (last, segments) = used.split_last()?;
        if last != name || segments.is_empty() {
            return None;
        }
        namespace_at(&self.namespaces, segments)
    }

    /// Walk a namespace path from `base`, not from the root.
    pub(in crate::compiler::resolve_names) fn namespace_at_from(
        &self,
        base: NamespaceId,
        path: &[String],
    ) -> Option<NamespaceId> {
        let mut current = base;
        for segment in path {
            current = *self.namespaces[current.0].children.get(segment)?;
        }
        Some(current)
    }

    /// Every declaration a bare name sees: the current namespace's own, then
    /// the declarations a name-`use` brings in. A namespace `use` does not
    /// bring its names bare.
    pub(in crate::compiler::resolve_names) fn bare_declarations(
        &self,
        namespace: NamespaceId,
        name: &str,
    ) -> Vec<DeclarationId> {
        let mut found: Vec<DeclarationId> = self.namespaces[namespace.0].get(name).to_vec();
        for used in &self.namespaces[namespace.0].uses {
            let Some(base) = self.name_use(used, name) else {
                continue;
            };
            for declaration in self.namespaces[base.0].get(name) {
                if self.skeletons[declaration.0].public && !found.contains(declaration) {
                    found.push(*declaration);
                }
            }
        }
        found
    }

    /// The declared type a bare name sees: the current namespace's own, then a
    /// type a name-`use` brings in.
    fn bare_type(&self, namespace: NamespaceId, name: &str) -> Option<TypeId> {
        if let Some(found) = self.namespaces[namespace.0].type_id(name) {
            return Some(found);
        }
        for used in &self.namespaces[namespace.0].uses {
            let Some(base) = self.name_use(used, name) else {
                continue;
            };
            if let Some(found) = self.namespaces[base.0].type_id(name)
                && self.types[found.0].public
            {
                return Some(found);
            }
        }
        None
    }

    /// Find a declared type by a path. A bare single segment reads the current
    /// namespace and the names it `use`s; a qualified path starts at a `use`d
    /// namespace and walks into it. A type reached across a namespace must be
    /// `pub`.
    pub(in crate::compiler::resolve_names) fn find_type(
        &self,
        namespace: NamespaceId,
        path: &[String],
    ) -> Option<TypeId> {
        let (name, prefix) = path
            .split_last()
            .expect("a type path has at least one segment");
        if prefix.is_empty() {
            return self.bare_type(namespace, name);
        }
        let base = self.used_namespace(namespace, &prefix[0])?;
        let rest = &prefix[1..];
        let target = self.namespace_at_from(base, rest)?;
        let found = self.namespaces[target.0].type_id(name)?;
        (target == namespace || self.types[found.0].public).then_some(found)
    }
}

/// A written type name: its path and its span. Every written name resolves
/// through one helper, which takes this.
struct WrittenType<'a> {
    path: &'a [String],
    span: &'a Span,
}

/// Collect the declarations an initializer references at evaluation time,
/// without descending into a function literal: a reference inside a function
/// body happens when the function is called, not when the value is built.
fn collect_initializer_references(expression: &Expression, out: &mut Vec<DeclarationId>) {
    match expression {
        Expression::Reference { declaration, .. } => out.push(*declaration),
        Expression::Text { .. } | Expression::Bool { .. } | Expression::Int { .. } => {}
        Expression::Map { fields, .. } => {
            for field in fields {
                collect_initializer_references(&field.value, out);
            }
        }
        Expression::Interpolated { parts, .. } => {
            for part in parts {
                if let StringPart::Expression(expression) = part {
                    collect_initializer_references(expression, out);
                }
            }
        }
        Expression::Call { arguments, .. } => {
            // A callee names a function or a native, never a module value, so
            // only the arguments are evaluated here.
            for argument in arguments {
                collect_initializer_references(argument, out);
            }
        }
        Expression::Field { target, .. } => collect_initializer_references(target, out),
        Expression::Variant { payload, .. } => {
            for element in payload {
                collect_initializer_references(element, out);
            }
        }
        Expression::Binary { left, right, .. } => {
            collect_initializer_references(left, out);
            collect_initializer_references(right, out);
        }
        Expression::Unary { operand, .. } => collect_initializer_references(operand, out),
        Expression::Match {
            subject,
            cases,
            default,
            ..
        } => {
            collect_initializer_references(subject, out);
            for arm in cases {
                if let crate::model::Pattern::Value(expression) = &arm.pattern {
                    collect_initializer_references(expression, out);
                }
                match &arm.body {
                    crate::model::MatchBody::Expression(expression) => {
                        collect_initializer_references(expression, out);
                    }
                    crate::model::MatchBody::Block(statements) => {
                        collect_initializer_references_in_statements(statements, out);
                    }
                }
            }
            if let Some(default) = default {
                match default {
                    crate::model::MatchBody::Expression(expression) => {
                        collect_initializer_references(expression, out);
                    }
                    crate::model::MatchBody::Block(statements) => {
                        collect_initializer_references_in_statements(statements, out);
                    }
                }
            }
        }
        // A function literal is code; its body runs when it is called.
        Expression::Function(_) => {}
    }
}

/// Collect references in a statement block, without descending into a nested
/// function literal.
fn collect_initializer_references_in_statements(
    statements: &[crate::model::Statement],
    out: &mut Vec<DeclarationId>,
) {
    use crate::model::Statement;
    for statement in statements {
        match statement {
            Statement::Bind { value, .. }
            | Statement::Assign { value, .. }
            | Statement::FieldAssign { value, .. }
            | Statement::Expression(value) => collect_initializer_references(value, out),
            Statement::Return { value, .. } => {
                if let Some(value) = value {
                    collect_initializer_references(value, out);
                }
            }
            Statement::Async { call, .. } => collect_initializer_references(call, out),
            Statement::Forever { body, .. } => {
                collect_initializer_references_in_statements(body, out);
            }
            Statement::Panic { .. } | Statement::Wait { .. } | Statement::Break { .. } => {}
        }
    }
}
