//! Registering the joined item stream.

use crate::compiler::Diagnostic;
use crate::compiler::load_files::{LoadedProgram, ScopedDeclaration};
use crate::compiler::model::{File, FileId, NamespaceId, Origin, Type, TypeHead, scalar_from_name};
use crate::native_registry::BUILTIN_NAMESPACE;
use crate::syntax::Span;

use super::builder::{Builder, DeclarationSite, Pending};

impl Builder {
    /// Reserve every declared type and record every `use`, before any
    /// declaration is registered. A model file is created for each loaded file,
    /// for diagnostics. Reserving the types first lets a native see the
    /// declared absence type when its row names it.
    pub(in crate::compiler::resolve_names) fn reserve_items(
        &mut self,
        loaded: &LoadedProgram,
    ) -> Result<(), Diagnostic> {
        for file in &loaded.files {
            self.files.push(File {
                path: file.path.clone(),
                declarations: Vec::new(),
            });
        }
        // Reserve every declared type before any field is resolved, so a type
        // may name itself and any other type, whatever the declaration order.
        for scoped in &loaded.items {
            let declaration = &scoped.declaration;
            if declaration.kind.declares_type() {
                let type_declaration = declaration
                    .type_declaration()
                    .expect("a type declaration carries its body");
                let namespace = self.ensure_namespace(&scoped.namespace);
                let group = declaration
                    .kind
                    .group()
                    .expect("a type declaration belongs to a type group");
                self.reserve_type(
                    type_declaration,
                    declaration.public,
                    group,
                    namespace,
                    &loaded.files[scoped.file].path,
                )?;
            }
        }
        // Record every `use` on the namespace that wrote it, before any field
        // is resolved: a field or a receiver type may name a type the namespace
        // only `use`s. A `use` is namespace-scoped, so it covers every
        // declaration of the namespace, and resolution is order-free, so there
        // is no position to check.
        for used in &loaded.uses {
            let namespace = self.ensure_namespace(&used.namespace);
            self.namespaces[namespace.0].uses.push(used.path.clone());
        }
        Ok(())
    }

    /// Register every declaration in the joined item stream, in order.
    pub(in crate::compiler::resolve_names) fn register_declarations(
        &mut self,
        loaded: &LoadedProgram,
    ) -> Result<(), Diagnostic> {
        for (item, scoped) in loaded.items.iter().enumerate() {
            self.register_scoped(loaded, item, scoped)?;
        }
        Ok(())
    }

    /// Register one declaration in its namespace. `item` is its index in the
    /// joined stream, so the linking pass finds its body again.
    fn register_scoped(
        &mut self,
        loaded: &LoadedProgram,
        item: usize,
        scoped: &ScopedDeclaration,
    ) -> Result<(), Diagnostic> {
        let loaded_file = &loaded.files[scoped.file];
        let declaration = &scoped.declaration;
        debug_assert!(
            declaration.kind.has_name(),
            "every registered declaration introduces a name"
        );
        if loaded_file.origin != Origin::Embedded
            && scoped.namespace.first().map(String::as_str) == BUILTIN_NAMESPACE.first().copied()
        {
            return Err(Diagnostic::new(
                &loaded_file.path,
                declaration.name_span,
                "`std` is reserved and cannot be declared",
            ));
        }
        let namespace = self.ensure_namespace(&scoped.namespace);
        // A declared type has no body and no node; its name was reserved in the
        // first pass, so its fields fill in here.
        if declaration.kind.declares_type() {
            let type_declaration = declaration
                .type_declaration()
                .expect("a type declaration carries its body");
            let id = self.namespaces[namespace.0]
                .types
                .get(&declaration.name)
                .expect("every declared type was reserved");
            let group = declaration
                .kind
                .group()
                .expect("a type declaration belongs to a type group");
            self.fill_type(type_declaration, id, group, namespace, &loaded_file.path)?;
            return Ok(());
        }
        // A function inside an `impl` block is filed under the block's type: a
        // function whose first parameter has the block's type head is a method,
        // and every other function is an associated function. A declaration
        // outside a block is filed under no type, so it is never reached
        // through one.
        if declaration.kind.callable() {
            let function = declaration
                .function()
                .expect("a callable declaration carries its function body");
            self.check_type_parameters(
                &function.type_parameters,
                function.name_span,
                &loaded_file.path,
            )?;
        }
        let (receiver_head, associated_head) = if declaration.kind.is_bound() {
            let function = declaration
                .function()
                .expect("a bound declaration carries its function body");
            let target = scoped
                .impl_target
                .as_ref()
                .expect("a bound declaration carries its impl target");
            let block_head =
                self.resolve_impl_target(&target.name, target.span, namespace, &loaded_file.path)?;
            let first_parameter = match function.parameters.first() {
                Some(parameter) => Some(
                    self.resolve_type_expr(
                        &parameter.ty,
                        namespace,
                        &function.type_parameters,
                        &loaded_file.path,
                    )?
                    .head(&self.types),
                ),
                None => None,
            };
            if first_parameter == Some(block_head) {
                (Some(block_head), None)
            } else {
                (None, Some(block_head))
            }
        } else {
            (None, None)
        };
        let file = FileId(scoped.file);
        let id = self.declare(DeclarationSite {
            kind: declaration.kind,
            namespace,
            name: &declaration.name,
            name_span: declaration.name_span,
            public: declaration.public,
            receiver_head,
            associated_head,
            file: Some(file),
            syntax_index: Some(item),
            reported_path: &loaded_file.path,
        })?;
        self.pending.push(Pending {
            declaration: id,
            file,
            syntax_index: item,
            namespace,
        });
        self.files[file.0].declarations.push(id);
        Ok(())
    }

    /// Resolve an `impl` block's target type name to the head every function
    /// in the block is filed under. The name resolves as a written type does: a
    /// scalar names the runtime's type, and a declared name is found in the
    /// file's namespace and the names it `use`s. Only a kind whose group binds
    /// methods may be an `impl` target.
    fn resolve_impl_target(
        &self,
        name: &str,
        span: Span,
        namespace: NamespaceId,
        reported_path: &std::path::Path,
    ) -> Result<TypeHead, Diagnostic> {
        let ty = match scalar_from_name(name) {
            Some(scalar) => scalar,
            None => {
                let path = [name.to_owned()];
                match self.find_type(namespace, &path) {
                    Some(id) => Type::Named(id, Vec::new()),
                    None => {
                        return Err(Diagnostic::new(
                            reported_path,
                            span,
                            format!("unknown type `{name}`"),
                        ));
                    }
                }
            }
        };
        if !ty.group(&self.types).binds_methods() {
            return Err(Diagnostic::new(
                reported_path,
                span,
                format!("`{name}` cannot carry methods"),
            ));
        }
        Ok(ty.head(&self.types))
    }
}
