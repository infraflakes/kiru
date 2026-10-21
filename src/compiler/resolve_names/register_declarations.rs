//! Registering the joined item stream.

use crate::compiler::Diagnostic;
use crate::compiler::load_files::{LoadedProgram, ScopedDeclaration};
use crate::compiler::model::{File, FileId};

use super::builder::{Builder, DeclarationSite, Pending};

impl Builder {
    /// Reserve every declared type, before any declaration is registered. A
    /// model file is created for each loaded file, for diagnostics. Reserving
    /// the types first lets a native row name a declared type by its path.
    pub(in crate::compiler::resolve_names) fn reserve_items(
        &mut self,
        loaded: &LoadedProgram,
    ) -> Result<(), Diagnostic> {
        for file in &loaded.files {
            self.ensure_namespace(&file.namespace);
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
                self.reserve_type(
                    type_declaration,
                    declaration.public,
                    namespace,
                    FileId(scoped.file),
                    &loaded.files[scoped.file].path,
                )?;
            }
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
        let namespace = self.ensure_namespace(&scoped.namespace);
        // A declared type has no body and no node; its name was reserved in the
        // first pass, so its fields fill in here.
        if declaration.kind.declares_type() {
            let type_declaration = declaration
                .type_declaration()
                .expect("a type declaration carries its body");
            let id = self
                .intern
                .resolve(&declaration.name)
                .and_then(|symbol| self.namespaces[namespace.0].types.get(symbol))
                .expect("every declared type was reserved");
            let group = declaration
                .kind
                .group()
                .expect("a type declaration belongs to a type group");
            self.fill_type(
                FileId(scoped.file),
                type_declaration,
                id,
                group,
                namespace,
                &loaded_file.path,
            )?;
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
            let block_head = self.resolve_impl_target(
                &target.ty,
                namespace,
                FileId(scoped.file),
                &loaded_file.path,
            )?;
            let first_parameter = match function.parameters.first() {
                Some(parameter) => Some(
                    self.resolve_type_expr(
                        &parameter.ty,
                        namespace,
                        FileId(scoped.file),
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
            reported_path: &loaded_file.path,
        })?;
        let pending = if declaration.kind.callable() {
            Pending::Callable {
                declaration: id,
                file,
                item,
                namespace,
            }
        } else {
            Pending::ModuleValue {
                declaration: id,
                file,
                item,
                namespace,
            }
        };
        self.pending.push(pending);
        self.files[file.0].declarations.push(id);
        Ok(())
    }
}
