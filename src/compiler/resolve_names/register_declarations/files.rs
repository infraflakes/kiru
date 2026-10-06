//! Registering a parsed file's declarations.

use crate::compiler::Diagnostic;
use crate::compiler::load_files::LoadedProgram;
use crate::model::{File, FileId, Origin, Registry};
use crate::native_registry::BUILTIN_NAMESPACE;
use crate::syntax::Declaration as ParsedDeclaration;
use crate::syntax::Span;

use super::builder::{Builder, DeclarationSite, Pending};

impl Builder {
    pub(in crate::compiler::resolve_names) fn register_file(
        &mut self,
        loaded: &LoadedProgram,
        index: usize,
        parsed: &[ParsedDeclaration],
    ) -> Result<(), Diagnostic> {
        let loaded_file = &loaded.files[index];
        let namespace_segments: &[String] = loaded_file
            .file
            .module
            .as_ref()
            .map(|module| module.segments.as_slice())
            .unwrap_or(&[]);
        if loaded_file.origin != Origin::Embedded
            && namespace_segments.first().map(String::as_str) == BUILTIN_NAMESPACE.first().copied()
        {
            let span = loaded_file
                .file
                .module
                .as_ref()
                .map(|module| module.span)
                .unwrap_or_else(|| Span::new(0, 0));
            return Err(Diagnostic::new(
                &loaded_file.path,
                span,
                "`std` is reserved and cannot be declared",
            ));
        }
        let namespace = self.ensure_namespace(namespace_segments);
        let file = FileId(self.files.len());
        self.files.push(File {
            path: loaded_file.path.clone(),
            namespace,
            imports: loaded_file.imports.iter().map(|i| FileId(*i)).collect(),
            declarations: Vec::new(),
        });

        for (syntax_index, declaration) in parsed.iter().enumerate() {
            let (name, name_span, registry) = match declaration {
                ParsedDeclaration::Function(function) => {
                    (&function.name, function.name_span, Registry::Function)
                }
                ParsedDeclaration::Binding(binding) => {
                    (&binding.name, binding.name_span, Registry::Value)
                }
            };
            let id = self.declare(
                DeclarationSite {
                    namespace,
                    name,
                    name_span,
                    file: Some(file),
                    syntax_index: Some(syntax_index),
                    reported_path: &loaded_file.path,
                },
                registry,
            )?;
            self.pending.push(Pending {
                declaration: id,
                file,
                syntax_index,
                namespace,
                order: self.skeletons[id.0].order,
            });
            self.files[file.0].declarations.push(id);
        }
        Ok(())
    }
}
