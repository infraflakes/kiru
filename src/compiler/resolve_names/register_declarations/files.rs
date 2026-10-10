//! Registering the joined item stream.

use crate::compiler::Diagnostic;
use crate::compiler::load_files::{LoadedProgram, ScopedDeclaration};
use crate::model::{File, FileId, Origin, Registry};
use crate::native_registry::BUILTIN_NAMESPACE;
use crate::syntax::Declaration as ParsedDeclaration;
use crate::syntax::Span;

use super::builder::{Builder, DeclarationSite, Pending};

impl Builder {
    /// Register every declaration in the joined item stream, in order. A model
    /// file is created for each loaded file, for diagnostics.
    pub(in crate::compiler::resolve_names) fn register_items(
        &mut self,
        loaded: &LoadedProgram,
    ) -> Result<(), Diagnostic> {
        for file in &loaded.files {
            self.files.push(File {
                path: file.path.clone(),
                declarations: Vec::new(),
            });
        }
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
        if loaded_file.origin != Origin::Embedded
            && scoped.namespace.first().map(String::as_str) == BUILTIN_NAMESPACE.first().copied()
        {
            return Err(Diagnostic::new(
                &loaded_file.path,
                declaration_span(&scoped.declaration),
                "`std` is reserved and cannot be declared",
            ));
        }
        let namespace = self.ensure_namespace(&scoped.namespace);
        let (name, name_span, registry) = match &scoped.declaration {
            ParsedDeclaration::Function(function) => {
                (&function.name, function.name_span, Registry::Function)
            }
            ParsedDeclaration::Binding(binding) => {
                (&binding.name, binding.name_span, Registry::Value)
            }
        };
        let file = FileId(scoped.file);
        let id = self.declare(
            DeclarationSite {
                namespace,
                name,
                name_span,
                file: Some(file),
                syntax_index: Some(item),
                reported_path: &loaded_file.path,
            },
            registry,
        )?;
        self.pending.push(Pending {
            declaration: id,
            file,
            syntax_index: item,
            namespace,
            order: self.skeletons[id.0].order,
        });
        self.files[file.0].declarations.push(id);
        Ok(())
    }
}

/// The span a declaration names, for a diagnostic.
fn declaration_span(declaration: &ParsedDeclaration) -> Span {
    match declaration {
        ParsedDeclaration::Function(function) => function.name_span,
        ParsedDeclaration::Binding(binding) => binding.name_span,
    }
}
