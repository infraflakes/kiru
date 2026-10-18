//! Registering the joined item stream.

use crate::compiler::Diagnostic;
use crate::compiler::load_files::{LoadedProgram, ScopedDeclaration};
use crate::model::{File, FileId, Origin, join_path, namespace_at};
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

    /// Reject a `use` whose path reaches neither a declared namespace nor a
    /// declaration: such a path names nothing, so what it claims to bring into
    /// scope can never resolve. Every namespace and declaration is registered
    /// by the time this runs.
    pub(in crate::compiler::resolve_names) fn check_uses(
        &self,
        loaded: &LoadedProgram,
    ) -> Result<(), Diagnostic> {
        for used in &loaded.uses {
            if self.use_reaches(&used.path) {
                continue;
            }
            return Err(Diagnostic::new(
                &loaded.files[used.file].path,
                used.span,
                format!(
                    "`{}` names neither a namespace nor a declaration",
                    join_path(&used.path)
                ),
            ));
        }
        Ok(())
    }

    /// Whether a `use` path reaches a declared namespace, or a declaration (or
    /// declared type) its last segment names in the namespace its prefix
    /// names.
    fn use_reaches(&self, path: &[String]) -> bool {
        if namespace_at(&self.namespaces, path).is_some() {
            return true;
        }
        let Some((name, prefix)) = path.split_last() else {
            return false;
        };
        if prefix.is_empty() {
            return false;
        }
        namespace_at(&self.namespaces, prefix).is_some_and(|base| {
            self.namespaces[base.0].names.contains(name)
                || self.namespaces[base.0].types.get(name).is_some()
        })
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
                // A module value is private to its namespace: a value is
                // evaluated at startup, so it has no cross-namespace form.
                if binding.public {
                    return Err(Diagnostic::new(
                        &loaded_file.path,
                        binding.name_span,
                        "a module value cannot be `pub`; it is private to its namespace",
                    ));
                }
                (&binding.name, binding.name_span, binding.public)
            }
            ParsedDeclaration::Type(_) => unreachable!("a type is handled above"),
        };
        // A function's first parameter is the receiver a method call matches:
        // resolve its head now, before any body is linked. A function is also
        // a constructor reachable through the type it returns, so resolve the
        // result's head too: a function whose first parameter is the receiver
        // of its own result is a method only, and every other function with a
        // result head answers a `Type.name(...)` call by that head.
        let (first_parameter_head, constructor_head) = match &scoped.declaration {
            ParsedDeclaration::Function(function) => {
                self.check_type_parameters(
                    &function.type_parameters,
                    function.name_span,
                    &loaded_file.path,
                )?;
                let receiver = match function.parameters.first() {
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
                };
                let constructor = match &function.return_type {
                    Some(return_type) => {
                        let head = self
                            .resolve_type_expr(
                                return_type,
                                namespace,
                                &function.type_parameters,
                                &loaded_file.path,
                            )?
                            .head();
                        // A function whose first parameter is the receiver of
                        // its own result is a method only: `with(self: Set, …)`
                        // is reached as `s.with(…)`, never as `Set.with(s, …)`,
                        // so a constructor of the same name stays unambiguous.
                        (receiver != Some(head)).then_some(head)
                    }
                    None => None,
                };
                (receiver, constructor)
            }
            ParsedDeclaration::Binding(_) => (None, None),
            ParsedDeclaration::Type(_) => unreachable!("a type is handled above"),
        };
        let file = FileId(scoped.file);
        let id = self.declare(DeclarationSite {
            namespace,
            name,
            name_span,
            public,
            first_parameter_head,
            constructor_head,
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
