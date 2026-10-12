//! Registering the joined item stream.

use std::collections::HashMap;

use crate::compiler::Diagnostic;
use crate::compiler::load_files::{LoadedProgram, ScopedDeclaration};
use crate::model::{File, FileId, Origin};
use crate::native_registry::BUILTIN_NAMESPACE;
use crate::syntax::Declaration as ParsedDeclaration;
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
            if let ParsedDeclaration::Type(declaration) = &scoped.declaration {
                let namespace = self.ensure_namespace(&scoped.namespace);
                self.reserve_type(declaration, namespace, &loaded.files[scoped.file].path)?;
            }
        }
        // The file that declares each namespace and the position of its first
        // declaration in the joined stream. A `use` may only reach a namespace
        // declared before it — the same backwards rule the resolver applies to
        // a name, extended across files. The declaring file names the other
        // file when a `use` reaches forward.
        let mut declared: HashMap<&[String], (usize, usize)> = HashMap::new();
        for (position, scoped) in loaded.items.iter().enumerate() {
            if !scoped.namespace.is_empty() {
                declared
                    .entry(scoped.namespace.as_slice())
                    .or_insert((scoped.file, position));
            }
        }
        // Record every `use` on the namespace that wrote it, before any field
        // is resolved: a field or a receiver type may name a type the
        // namespace only `use`s.
        for used in &loaded.uses {
            // A use names a namespace, or one declaration inside it: the
            // longest prefix that is a declared namespace is the one reached.
            let reached = (1..=used.path.len())
                .rev()
                .find_map(|length| declared.get(&used.path[..length]).copied());
            if let Some((declaring_file, position)) = reached
                && position >= used.declarations_before
            {
                return Err(Diagnostic::new(
                    &loaded.files[used.file].path,
                    used.span,
                    format!(
                        "`{}` is declared in `{}`, which is loaded after `{}`",
                        crate::model::join_path(&used.path, false),
                        file_name(&loaded.files[declaring_file].path),
                        file_name(&loaded.files[used.file].path),
                    ),
                ));
            }
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
        // A declared type has no body and no node; its name was reserved in the
        // first pass, so its fields fill in here.
        if let ParsedDeclaration::Type(declaration) = &scoped.declaration {
            let id = self.namespaces[namespace.0]
                .types
                .get(&declaration.name)
                .expect("every declared type was reserved");
            self.fill_type(declaration, id, namespace, &loaded_file.path)?;
            return Ok(());
        }
        let (name, name_span, public) = match &scoped.declaration {
            ParsedDeclaration::Function(function) => {
                (&function.name, function.name_span, function.public)
            }
            ParsedDeclaration::Binding(binding) => {
                (&binding.name, binding.name_span, binding.public)
            }
            ParsedDeclaration::Type(_) => unreachable!("a type is handled above"),
        };
        // A function's first parameter is the receiver a method call matches:
        // resolve its head now, before any body is linked, so a forward
        // reference can still dispatch on it and be rejected for order.
        let first_parameter_head = match &scoped.declaration {
            ParsedDeclaration::Function(function) => {
                self.check_type_parameters(
                    &function.type_parameters,
                    function.name_span,
                    &loaded_file.path,
                )?;
                match function.parameters.first() {
                    Some(parameter) => Some(
                        self.resolve_type_expr(
                            &parameter.ty,
                            namespace,
                            &function.type_parameters,
                            &loaded_file.path,
                        )?
                        .head(),
                    ),
                    None => None,
                }
            }
            ParsedDeclaration::Binding(_) => None,
            ParsedDeclaration::Type(_) => unreachable!("a type is handled above"),
        };
        let file = FileId(scoped.file);
        let id = self.declare(DeclarationSite {
            namespace,
            name,
            name_span,
            public,
            first_parameter_head,
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
}

/// The span a declaration names, for a diagnostic.
fn declaration_span(declaration: &ParsedDeclaration) -> Span {
    match declaration {
        ParsedDeclaration::Function(function) => function.name_span,
        ParsedDeclaration::Binding(binding) => binding.name_span,
        ParsedDeclaration::Type(type_declaration) => type_declaration.name_span,
    }
}

/// The last path segment of a loaded file, for a diagnostic that names the
/// declaring and writing files without their directories.
fn file_name(path: &std::path::Path) -> String {
    path.file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.display().to_string())
}
