//! Recording one loaded file: parsing it and filing its declarations and uses.

use std::path::PathBuf;

use super::embedded::EMBEDDED;
use super::tree::{STD_TREE, namespace_of};
use crate::compiler::Diagnostic;
use crate::compiler::compile::load::load_error::LoadError;
use crate::compiler::compile::load::loaded_program::{
    ImplTarget, LoadedFile, LoadedProgram, ScopedDeclaration, ScopedUse,
};
use crate::compiler::model::{DeclarationKind, FileOrigin};
use crate::syntax;
use crate::syntax::{Declaration, DeclarationForm, Item, TypeDeclarationKind};

/// The accumulating state of a load: the files read so far, their joined
/// declarations and uses, the syntax errors recovery found, and the index of
/// the entry file once the walk reaches it.
pub(super) struct Loader {
    pub(super) files: Vec<LoadedFile>,
    pub(super) items: Vec<ScopedDeclaration>,
    pub(super) uses: Vec<ScopedUse>,
    pub(super) syntax: Vec<Diagnostic>,
}

impl Loader {
    pub(super) fn new() -> Self {
        Self {
            files: Vec::new(),
            items: Vec::new(),
            uses: Vec::new(),
            syntax: Vec::new(),
        }
    }

    /// Take the recorded files and their joined stream.
    pub(super) fn finish(self) -> LoadedProgram {
        LoadedProgram {
            files: self.files,
            items: self.items,
            uses: self.uses,
            syntax: self.syntax,
        }
    }

    /// Add the standard library from the table compiled into the binary.
    pub(super) fn add_embedded(&mut self) -> Result<(), LoadError> {
        for (relative, embedded_source) in EMBEDDED {
            let namespace = namespace_of(relative, STD_TREE);
            // The loaded path carries the tree's name for diagnostics; the
            // table itself is keyed by the file's path under the tree.
            let path = format!("<{STD_TREE}>/{relative}");
            self.add(
                PathBuf::from(path),
                FileOrigin::Embedded,
                (*embedded_source).to_owned(),
                namespace,
            )?;
        }
        Ok(())
    }

    /// Parse one file and record its declarations and uses in its namespace.
    /// The items that parsed are recorded even when recovery found syntax
    /// errors; a lexer error fails the load, because there is no token
    /// boundary to recover to.
    pub(super) fn add(
        &mut self,
        path: PathBuf,
        origin: FileOrigin,
        source: String,
        namespace: Vec<String>,
    ) -> Result<usize, LoadError> {
        let parsed = match syntax::parse_file(&source) {
            Ok(parsed) => parsed,
            Err(error) => {
                return Err(LoadError {
                    path,
                    span: error.span,
                    message: error.message,
                    source,
                });
            }
        };
        let index = self.files.len();
        for error in parsed.errors {
            self.syntax
                .push(Diagnostic::new(&path, error.span, error.message));
        }
        self.files.push(LoadedFile {
            path,
            origin,
            source,
            namespace: namespace.clone(),
        });
        for item in parsed.file.items {
            match item {
                Item::Declaration(declaration) => self.record(*declaration, index, &namespace),
                Item::Use(use_item) => {
                    for path in use_item.paths {
                        self.uses.push(ScopedUse {
                            file: index,
                            namespace: namespace.clone(),
                            path: path.segments,
                            path_span: path.span,
                            delivered: path.delivered,
                            span: use_item.span,
                        });
                    }
                }
            }
        }
        Ok(index)
    }

    /// Record one declaration. A bound form is the surface group an `impl`
    /// block writes: its members are the declarations, each carrying the impl
    /// kind the loader records and its block's target. Every other form is one
    /// declaration.
    fn record(&mut self, declaration: Declaration, file: usize, namespace: &[String]) {
        if !matches!(&declaration.form, DeclarationForm::Impl(_)) {
            let kind = declaration_kind(&declaration);
            self.items.push(ScopedDeclaration {
                file,
                namespace: namespace.to_owned(),
                kind,
                declaration,
                impl_target: None,
            });
            return;
        }
        let block = declaration
            .into_impl()
            .expect("a bound declaration form carries its impl block");
        let target = ImplTarget { ty: block.ty };
        for member in block.functions {
            self.items.push(ScopedDeclaration {
                file,
                namespace: namespace.to_owned(),
                kind: DeclarationKind::Impl,
                declaration: member,
                impl_target: Some(ImplTarget {
                    ty: target.ty.clone(),
                }),
            });
        }
    }
}

/// The model kind a loaded declaration form carries. The loader is the one
/// boundary that turns the surface form into a model kind; every later pass
/// reads the kind it recorded instead of matching the syntax tree.
fn declaration_kind(declaration: &Declaration) -> DeclarationKind {
    match &declaration.form {
        DeclarationForm::Function(_) => DeclarationKind::Function,
        DeclarationForm::Binding(_) => DeclarationKind::ModuleValue,
        DeclarationForm::Type(declaration) => match declaration.kind {
            TypeDeclarationKind::Rec(_) => DeclarationKind::Rec,
            TypeDeclarationKind::Enum(_) => DeclarationKind::Enum,
        },
        DeclarationForm::Impl(_) => {
            unreachable!("an impl block is flattened before it is recorded")
        }
    }
}
