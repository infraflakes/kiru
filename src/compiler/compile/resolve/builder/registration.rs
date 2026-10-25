//! Registering declaration nodes: reserving types, declaring names, and
//! filling a type's fields.

use std::cell::RefCell;
use std::collections::HashMap;

use crate::compiler::model::{
    Declaration, DeclarationId, DeclarationKind, File, FileId, FunctionBinding, Group, Interner,
    MethodTable, NameTable, Namespace, NamespaceId, Program, RuntimeType, Symbol, TypeBody,
    TypeFieldDecl, TypeKind, TypeTable, VariantDecl, first_duplicate,
};
use crate::compiler::model::{Type, TypeHead, TypeUse};
use crate::compiler::native_registry::Native;
use crate::compiler::{Diagnostic, duplicate_field, duplicate_in_namespace};
use crate::syntax::{Span, TypeDeclaration};

/// Where a file declaration is written, for registration.
pub(in crate::compiler::compile::resolve) struct DeclarationSite<'a> {
    pub(in crate::compiler::compile::resolve) kind: DeclarationKind,
    pub(in crate::compiler::compile::resolve) namespace: NamespaceId,
    pub(in crate::compiler::compile::resolve) name: &'a str,
    pub(in crate::compiler::compile::resolve) name_span: Span,
    /// Whether another namespace may reach the declaration.
    pub(in crate::compiler::compile::resolve) public: bool,
    /// The type head of the block this declaration is a method of, when it sits
    /// in an `impl` block and its first parameter has the block's type. What a
    /// `receiver.name(...)` call matches against.
    pub(in crate::compiler::compile::resolve) receiver_head: Option<TypeHead>,
    /// The type head of the block this declaration is an associated function
    /// of, when it sits in an `impl` block and its first parameter does not
    /// have the block's type. What a `Type.name(...)` call matches against.
    pub(in crate::compiler::compile::resolve) associated_head: Option<TypeHead>,
    pub(in crate::compiler::compile::resolve) file: Option<FileId>,
    pub(in crate::compiler::compile::resolve) reported_path: &'a std::path::Path,
}

/// What is known about a declaration before its body is linked.
pub(in crate::compiler::compile::resolve) struct Skeleton {
    /// The kind the model node will hold. A bound member enters registration
    /// as the loader's `Impl` form and is normalized to `Function` here, so
    /// no node in the program graph has kind `Impl`.
    pub(in crate::compiler::compile::resolve) kind: DeclarationKind,
    /// How the declaration is bound when it is a function: a namespace name,
    /// the body that writes the closure literal, a receiver head, or an
    /// associated function's type head. Every other kind has none.
    pub(in crate::compiler::compile::resolve) binding: Option<FunctionBinding>,
    pub(in crate::compiler::compile::resolve) name: String,
    pub(in crate::compiler::compile::resolve) name_span: Span,
    /// Whether another namespace may reach the declaration.
    pub(in crate::compiler::compile::resolve) public: bool,
    /// The type head of the block this declaration is a method of, when it sits
    /// in an `impl` block and its first parameter has the block's type. Known
    /// at registration so a forward method reference can be dispatched before
    /// its target is linked.
    pub(in crate::compiler::compile::resolve) receiver_head: Option<TypeHead>,
    /// The type head of the block this declaration is an associated function
    /// of, when it sits in an `impl` block and its first parameter does not
    /// have the block's type. Known at registration so a forward
    /// `Type.name(...)` reference can be dispatched before its target is
    /// linked.
    pub(in crate::compiler::compile::resolve) associated_head: Option<TypeHead>,
    pub(in crate::compiler::compile::resolve) file: Option<FileId>,
    pub(in crate::compiler::compile::resolve) namespace: NamespaceId,
}

impl Skeleton {
    /// Whether the declaration is filed under a type: a method or an
    /// associated function in an `impl` block, or a native whose row gives it a
    /// head. A filed declaration does not own its name alone; a plain function,
    /// a module value, and a type do. The heads decide, not the kind: a bound
    /// native has a head and no function binding.
    pub(in crate::compiler::compile::resolve) fn filed(&self) -> bool {
        self.receiver_head.is_some() || self.associated_head.is_some()
    }
}

/// A worklist entry for the linking pass: a declaration whose body comes from
/// one parsed item. Only a callable and a module value own a body, so the two
/// forms are the variants and the linking pass matches one instead of testing a
/// kind and falling back to an unreachable arm.
pub(in crate::compiler::compile::resolve) enum Pending {
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
    pub(in crate::compiler::compile::resolve) fn parts(
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
pub(in crate::compiler::compile::resolve) enum Slot {
    /// A parameter or local binding: storage for the declared type.
    Binding(Type),
    /// A native callable: its type and its runtime index.
    Native(Type, Native),
}

pub(in crate::compiler::compile::resolve) struct Builder {
    pub(in crate::compiler::compile::resolve) namespaces: Vec<Namespace>,
    pub(in crate::compiler::compile::resolve) skeletons: Vec<Skeleton>,
    /// A node for every declaration, indexed by `DeclarationId`. A callable or
    /// module value starts as a placeholder that the first linking pass fills
    /// before any body is linked, so every id a body can name has its node when
    /// the body is typed. A parameter and a local are created with their type.
    pub(in crate::compiler::compile::resolve) declarations: Vec<Declaration>,
    pub(in crate::compiler::compile::resolve) files: Vec<File>,
    /// The runtime list the compiler registers, once, before any native row
    /// names it.
    pub(in crate::compiler::compile::resolve) list_type: Option<DeclarationId>,
    pub(in crate::compiler::compile::resolve) pending: Vec<Pending>,
    /// Every module value, in declaration order. Registration is the one place
    /// that knows them, so ordering their initializers never scans the whole
    /// declaration table.
    pub(in crate::compiler::compile::resolve) module_values: Vec<DeclarationId>,
    /// The one interner every name-table key came from. Registration interns
    /// a declaration's, a namespace's, and a type's name exactly once.
    pub(in crate::compiler::compile::resolve) intern: Interner,
    /// Every written type name the resolution met, in source order.
    pub(in crate::compiler::compile::resolve) type_uses: RefCell<Vec<TypeUse>>,
}

impl Builder {
    pub(in crate::compiler::compile::resolve) fn new() -> Self {
        let root = Namespace {
            name: String::new(),
            parent: None,
            children: HashMap::new(),
            names: NameTable::default(),
            types: TypeTable::default(),
            methods: MethodTable::default(),
            associates: MethodTable::default(),
            brought: HashMap::new(),
        };
        Self {
            namespaces: vec![root],
            skeletons: Vec::new(),
            declarations: Vec::new(),
            files: Vec::new(),
            list_type: None,
            pending: Vec::new(),
            module_values: Vec::new(),
            intern: Interner::default(),
            type_uses: RefCell::new(Vec::new()),
        }
    }

    pub(in crate::compiler::compile::resolve) fn finish(
        self,
        module_values: Vec<DeclarationId>,
    ) -> Program {
        Program {
            files: self.files,
            namespaces: self.namespaces,
            declarations: self.declarations,
            list_type: self.list_type,
            module_values,
            intern: self.intern,
            type_uses: self.type_uses.into_inner(),
        }
    }

    /// The namespace node for a path, creating missing segments. Each segment
    /// names its parent's children by the symbol it is interned under, which
    /// is also the key every walk over the tree reads.
    pub(in crate::compiler::compile::resolve) fn ensure_namespace(
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
                        types: TypeTable::default(),
                        methods: MethodTable::default(),
                        associates: MethodTable::default(),
                        brought: HashMap::new(),
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
    pub(in crate::compiler::compile::resolve) fn create_node(
        &mut self,
        skeleton: Skeleton,
        owner: Option<DeclarationId>,
        cell: Option<Slot>,
    ) -> DeclarationId {
        let id = DeclarationId(self.skeletons.len());
        let kind = skeleton.kind;
        let binding = skeleton.binding;
        let name = skeleton.name.clone();
        let name_span = skeleton.name_span;
        let file = skeleton.file;
        let namespace = skeleton.namespace;
        let public = skeleton.public;
        self.skeletons.push(skeleton);
        let declaration = match cell {
            Some(Slot::Binding(ty)) => Declaration {
                kind,
                binding,
                name,
                name_span,
                public,
                file,
                namespace,
                owner,
                ty,
                value: None,
                native: None,
                type_body: None,
            },
            Some(Slot::Native(ty, native)) => Declaration {
                kind,
                binding,
                name,
                name_span,
                public,
                file,
                namespace,
                owner,
                ty,
                value: None,
                native: Some(native),
                type_body: None,
            },
            // A callable or module value: its phase fills the real type and
            // value. Until then it holds the placeholder type; no body is
            // linked before the first pass settles every one of them.
            None => Declaration {
                kind,
                binding,
                name,
                name_span,
                public,
                file,
                namespace,
                owner,
                ty: Type::Text,
                value: None,
                native: None,
                type_body: None,
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
    pub(in crate::compiler::compile::resolve) fn declare(
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
        // The loader's `Impl` kind is a surface fact: a bound function enters
        // registration as `Impl` and leaves as the model's one `Function` form,
        // with the head that files it carried as its binding. A plain function
        // is the named binding, and every other kind has no function binding.
        let (kind, binding) = match kind {
            DeclarationKind::Function => (DeclarationKind::Function, Some(FunctionBinding::Named)),
            DeclarationKind::Impl if receiver_head.is_some() => (
                DeclarationKind::Function,
                receiver_head.map(|receiver| FunctionBinding::Method { receiver }),
            ),
            DeclarationKind::Impl if associated_head.is_some() => (
                DeclarationKind::Function,
                associated_head.map(|owner| FunctionBinding::Associated { owner }),
            ),
            kind => (kind, None),
        };
        let id = self.create_node(
            Skeleton {
                kind,
                binding,
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
        // A namespace-level function or value owns its name in the value
        // pool. A filed function owns its name inside its type's impl scope
        // and never collides with the namespace-level pool. Methods and
        // associates share the impl scope, so the same type head and name
        // cannot be both.
        let duplicate = if filed {
            match (receiver_head, associated_head) {
                (Some(head), None) => self.insert_bound_function(namespace, head, symbol, id, true),
                (None, Some(head)) => {
                    self.insert_bound_function(namespace, head, symbol, id, false)
                }
                (None, None) => unreachable!("a filed declaration carries a type head"),
                (Some(_), Some(_)) => {
                    unreachable!("a declaration carries one binding head")
                }
            }
        } else {
            self.namespaces[namespace.0]
                .names
                .get(symbol)
                .map_or(Ok(()), Err)
        };
        if let Err(existing) = duplicate {
            let message = match receiver_head.or(associated_head) {
                Some(head) => format!(
                    "`{name}` is declared more than once for `{}`",
                    self.head_name(head)
                ),
                None => duplicate_in_namespace(name),
            };
            let mut diagnostic = Diagnostic::new(reported_path, name_span, message);
            if let Some(related_file) = self.skeletons[existing.0].file {
                diagnostic = diagnostic.with_related(
                    self.files[related_file.0].path.clone(),
                    self.skeletons[existing.0].name_span,
                    crate::compiler::diagnostic::first_declaration(name),
                );
            }
            return Err(diagnostic);
        }
        if !filed {
            self.namespaces[namespace.0].names.insert(symbol, id);
        }
        Ok(id)
    }

    /// The written name of a type head, for a diagnostic. A named type head
    /// names its declaration, a scalar head names its word, and the two
    /// structural heads have no declared name.
    fn head_name(&self, head: TypeHead) -> String {
        match head {
            TypeHead::Scalar(name) => (*name).to_owned(),
            TypeHead::Named(id) => self.declarations[id.0].name.clone(),
            TypeHead::Function => "fn".to_owned(),
            TypeHead::Parameter => "a type parameter".to_owned(),
        }
    }

    /// Register a method or associate in its type's impl scope. A method and
    /// an associate share that scope, so both pools are checked before the
    /// declaration joins one of them. The existing declaration is returned
    /// when the head and name are already taken.
    pub(in crate::compiler::compile::resolve) fn insert_bound_function(
        &mut self,
        namespace: NamespaceId,
        head: TypeHead,
        symbol: Symbol,
        id: DeclarationId,
        method: bool,
    ) -> Result<(), DeclarationId> {
        let table = &mut self.namespaces[namespace.0];
        if let Some(existing) = table
            .methods
            .get(head, symbol)
            .or_else(|| table.associates.get(head, symbol))
        {
            return Err(existing);
        }
        if method {
            debug_assert!(
                table.methods.insert(head, symbol, id).is_ok(),
                "the impl scope was checked before insertion"
            );
        } else {
            debug_assert!(
                table.associates.insert(head, symbol, id).is_ok(),
                "the impl scope was checked before insertion"
            );
        }
        Ok(())
    }

    /// Reserve the node a closure literal's value holds. The node owns no
    /// namespace name and joins no registry: its container is the function or
    /// module value whose body writes the literal, and its parameters and
    /// locals are owned by the node itself. Its type and value are filled when
    /// the literal is linked.
    pub(in crate::compiler::compile::resolve) fn declare_anonymous_function(
        &mut self,
        container: DeclarationId,
        file: FileId,
        namespace: NamespaceId,
        span: Span,
    ) -> DeclarationId {
        self.create_node(
            Skeleton {
                kind: DeclarationKind::Function,
                binding: Some(FunctionBinding::Anonymous { container }),
                name: "<closure>".to_owned(),
                name_span: span,
                public: false,
                receiver_head: None,
                associated_head: None,
                file: Some(file),
                namespace,
            },
            None,
            None,
        )
    }

    /// Whether `caller` may reach a bound function filed in `owner`: in its own
    /// namespace always, and across namespaces only when the function is `pub`
    /// and its type is `pub`. A bound function is never gated by a `use`. The
    /// one rule is the model's; the skeletons supply its facts.
    pub(in crate::compiler::compile::resolve) fn bound_reachable(
        &self,
        caller: NamespaceId,
        owner: NamespaceId,
        declaration: DeclarationId,
    ) -> bool {
        let skeleton = &self.skeletons[declaration.0];
        let type_public = match skeleton.receiver_head.or(skeleton.associated_head) {
            Some(TypeHead::Named(id)) => Some(self.declarations[id.0].public),
            _ => None,
        };
        crate::compiler::model::bound_reachable(skeleton.public, type_public, owner, caller)
    }

    /// Reject a repeated type parameter, whichever declaration carries it: a
    /// `rec`, an `enum`, or a function. Both the declared-type path and the
    /// function path check through this one scan.
    pub(in crate::compiler::compile::resolve) fn check_type_parameters(
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

    /// Reserve a declared type: check its parameters, create its declaration
    /// node, and register its one type name in its namespace's type pool,
    /// before any field type is resolved. Every declared type is reserved
    /// before any is filled, so a type may name itself and any other type,
    /// whatever the declaration order. The type pool is separate from the
    /// value names, so a type and a value may share a spelling. The node's
    /// type is the type constructor applied to its own parameters, and its
    /// body starts empty and is filled later.
    pub(in crate::compiler::compile::resolve) fn reserve_type(
        &mut self,
        declaration: &TypeDeclaration,
        kind: DeclarationKind,
        public: bool,
        namespace: NamespaceId,
        file: FileId,
        reported_path: &std::path::Path,
    ) -> Result<DeclarationId, Diagnostic> {
        self.check_type_parameters(
            &declaration.parameters,
            declaration.name_span,
            reported_path,
        )?;
        let body_kind = match kind {
            DeclarationKind::Rec => TypeKind::Rec(Vec::new()),
            DeclarationKind::Enum => TypeKind::Enum(Vec::new()),
            _ => unreachable!("only a type declaration reserves a type body"),
        };
        let id = self.create_node(
            Skeleton {
                kind,
                binding: None,
                name: declaration.name.clone(),
                name_span: declaration.name_span,
                public,
                receiver_head: None,
                associated_head: None,
                file: Some(file),
                namespace,
            },
            None,
            None,
        );
        let parameters = declaration.parameters.clone();
        self.declarations[id.0].ty = Type::Named(
            id,
            parameters.iter().cloned().map(Type::Parameter).collect(),
        );
        self.declarations[id.0].type_body = Some(TypeBody {
            parameters,
            runtime: RuntimeType::Declared,
            kind: body_kind,
        });
        if let Err(existing) = self.namespaces[namespace.0]
            .types
            .insert(self.intern.intern(&declaration.name), id)
        {
            let mut diagnostic = Diagnostic::new(
                reported_path,
                declaration.name_span,
                duplicate_in_namespace(&declaration.name),
            );
            if let Some(related_file) = self.skeletons[existing.0].file {
                diagnostic = diagnostic.with_related(
                    self.files[related_file.0].path.clone(),
                    self.skeletons[existing.0].name_span,
                    crate::compiler::diagnostic::first_declaration(&declaration.name),
                );
            }
            return Err(diagnostic);
        }
        Ok(id)
    }

    /// Fill a reserved declared type: resolve its field or variant types into
    /// its body.
    pub(in crate::compiler::compile::resolve) fn fill_type(
        &mut self,
        file: FileId,
        declaration: &TypeDeclaration,
        id: DeclarationId,
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
        self.declarations[id.0]
            .type_body
            .as_mut()
            .expect("a reserved type declaration carries its type body")
            .kind = kind;
        Ok(())
    }
}
