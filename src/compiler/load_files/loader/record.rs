//! Recording one loaded file: parsing it and filing its declarations and uses.

use std::path::PathBuf;

use super::embedded::EMBEDDED;
use super::tree::{STD_TREE, namespace_of};
use crate::compiler::load_files::load_error::LoadError;
use crate::compiler::load_files::loaded_program::{
    ImplTarget, LoadedFile, ScopedDeclaration, ScopedUse,
};
use crate::syntax;
use crate::syntax::{Declaration, Item};

/// The accumulating state of a load: the files read so far, their joined
/// declarations and uses, and the index of the entry file once the walk
/// reaches it.
pub(super) struct Loader {
    pub(super) files: Vec<LoadedFile>,
    pub(super) items: Vec<ScopedDeclaration>,
    pub(super) uses: Vec<ScopedUse>,
    /// The index of the entry file once the walk reaches it.
    pub(super) entry: Option<usize>,
}

impl Loader {
    pub(super) fn new() -> Self {
        Self {
            files: Vec::new(),
            items: Vec::new(),
            uses: Vec::new(),
            entry: None,
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
                (*embedded_source).to_owned(),
                namespace,
            )?;
        }
        Ok(())
    }

    /// Parse one file and record its declarations and uses in its namespace.
    pub(super) fn add(
        &mut self,
        path: PathBuf,
        source: String,
        namespace: Vec<String>,
    ) -> Result<usize, LoadError> {
        let file = match syntax::parse_file(&source) {
            Ok(file) => file,
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
        self.files.push(LoadedFile {
            path,
            source,
            namespace: namespace.clone(),
        });
        for item in file.items {
            match item {
                Item::Declaration(declaration) => self.record(*declaration, index, &namespace),
                Item::Use(use_item) => {
                    for (path, delivered) in use_item.paths.into_iter().zip(use_item.delivered) {
                        self.uses.push(ScopedUse {
                            file: index,
                            namespace: namespace.clone(),
                            path,
                            delivered,
                            span: use_item.span,
                        });
                    }
                }
            }
        }
        Ok(index)
    }

    /// Record one declaration. A bound form is the surface group an `impl`
    /// block writes: its members are the declarations, each already marked with
    /// the impl kind and carrying its block's target. Every other form is one
    /// declaration.
    fn record(&mut self, declaration: Declaration, file: usize, namespace: &[String]) {
        if !declaration.kind.is_bound() {
            self.items.push(ScopedDeclaration {
                file,
                namespace: namespace.to_owned(),
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
                declaration: member,
                impl_target: Some(ImplTarget {
                    ty: target.ty.clone(),
                }),
            });
        }
    }
}
