//! Reading the entry file and every file it imports.
//!
//! Loading joins every file into one ordered item stream: a file's items in
//! order, with each `import` replaced by the imported file's items. A file is
//! loaded once; importing it a second time, or importing in a cycle, is an
//! error.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::model::Origin;
use crate::syntax;
use crate::syntax::{Item, Module, Span};

use super::load_error::LoadError;
use super::loaded_program::{LoadedFile, LoadedProgram, ScopedDeclaration};
use super::resolve_import_paths::resolve_import;

/// The standard library, compiled into every program and always visible.
///
/// These files are seeded before the entry so their declarations come first.
pub(crate) const EMBEDDED: &[(&str, &str)] = &[
    ("<std>/std.kiru", include_str!("../../../stdlib/std.kiru")),
    (
        "<std>/lists.kiru",
        include_str!("../../../stdlib/lists.kiru"),
    ),
    ("<std>/text.kiru", include_str!("../../../stdlib/text.kiru")),
    ("<std>/fs.kiru", include_str!("../../../stdlib/fs.kiru")),
    ("<std>/env.kiru", include_str!("../../../stdlib/env.kiru")),
    ("<std>/path.kiru", include_str!("../../../stdlib/path.kiru")),
    (
        "<std>/process.kiru",
        include_str!("../../../stdlib/process.kiru"),
    ),
];

/// Load the entry file and everything it imports.
pub(crate) fn load_files(entry: &Path) -> Result<LoadedProgram, LoadError> {
    let entry_path = entry.canonicalize().map_err(|_| LoadError {
        path: entry.to_path_buf(),
        span: Span::new(0, 0),
        message: format!("cannot find {}", entry.display()),
        source: String::new(),
    })?;

    let mut loader = Loader {
        files: Vec::new(),
        items: Vec::new(),
        loaded: HashMap::new(),
        loading: HashSet::new(),
    };
    loader.seed_embedded()?;
    let entry_index = loader.load_file(&entry_path, Span::new(0, 0), entry)?;
    Ok(LoadedProgram {
        entry: entry_index,
        files: loader.files,
        items: loader.items,
    })
}

struct Loader {
    files: Vec<LoadedFile>,
    items: Vec<ScopedDeclaration>,
    /// The index of every file already loaded, for the duplicate-import check.
    loaded: HashMap<PathBuf, usize>,
    /// The files being loaded, for the cycle check.
    loading: HashSet<PathBuf>,
}

impl Loader {
    /// Parse and register the standard library. A failure here is a compiler
    /// bug, reported like any other load error instead of aborting.
    fn seed_embedded(&mut self) -> Result<(), LoadError> {
        for (path, source) in EMBEDDED {
            let file = syntax::parse_file(source).map_err(|error| LoadError {
                path: PathBuf::from(path),
                span: error.span,
                message: format!(
                    "embedded standard library failed to parse: {}",
                    error.message
                ),
                source: (*source).to_owned(),
            })?;
            let index = self.files.len();
            self.files.push(LoadedFile {
                path: PathBuf::from(path),
                source: (*source).to_owned(),
                origin: Origin::Embedded,
            });
            for item in file.items {
                self.push_item(index, item);
            }
        }
        Ok(())
    }

    /// Append one non-import item to the joined stream: a declaration at the
    /// root, or a `mod` block's declarations in its namespace.
    fn push_item(&mut self, file: usize, item: Item) {
        match item {
            Item::Declaration(declaration) => self.items.push(ScopedDeclaration {
                file,
                namespace: Vec::new(),
                declaration,
            }),
            Item::Module(module) => self.push_module(file, module),
            // Imports are expanded by `load_file`, which never routes one here.
            Item::Import(_) => {}
        }
    }

    fn push_module(&mut self, file: usize, module: Module) {
        for declaration in module.declarations {
            self.items.push(ScopedDeclaration {
                file,
                namespace: module.path.clone(),
                declaration,
            });
        }
    }

    fn load_file(
        &mut self,
        canonical: &Path,
        span: Span,
        reported: &Path,
    ) -> Result<usize, LoadError> {
        if self.loaded.contains_key(canonical) {
            return Err(LoadError {
                path: reported.to_path_buf(),
                span,
                message: format!("`{}` is imported more than once", canonical.display()),
                source: String::new(),
            });
        }
        if self.loading.contains(canonical) {
            return Err(LoadError {
                path: reported.to_path_buf(),
                span,
                message: format!("import cycle through {}", canonical.display()),
                source: String::new(),
            });
        }

        let source = std::fs::read_to_string(canonical).map_err(|error| LoadError {
            path: reported.to_path_buf(),
            span,
            message: format!("cannot read {}: {error}", canonical.display()),
            source: String::new(),
        })?;
        let file = match syntax::parse_file(&source) {
            Ok(file) => file,
            Err(error) => {
                return Err(LoadError {
                    path: canonical.to_path_buf(),
                    span: error.span,
                    message: error.message,
                    source,
                });
            }
        };

        let directory = canonical.parent().unwrap_or(Path::new(".")).to_path_buf();
        let index = self.files.len();
        self.files.push(LoadedFile {
            path: canonical.to_path_buf(),
            source,
            origin: Origin::File,
        });
        self.loading.insert(canonical.to_path_buf());
        for item in file.items {
            match item {
                Item::Import(import) => {
                    let Some(target) = resolve_import(&directory, &import.path) else {
                        return Err(LoadError {
                            path: canonical.to_path_buf(),
                            span: import.span,
                            message: format!("cannot find import {}", import.path),
                            source: String::new(),
                        });
                    };
                    match self.load_file(&target, import.span, canonical) {
                        Ok(_) => {}
                        Err(error) if error.source.is_empty() => {
                            return Err(LoadError {
                                path: error.path,
                                span: error.span,
                                message: error.message,
                                source: self.files[index].source.clone(),
                            });
                        }
                        Err(error) => return Err(error),
                    }
                }
                other => self.push_item(index, other),
            }
        }
        self.loading.remove(canonical);
        self.loaded.insert(canonical.to_path_buf(), index);
        Ok(index)
    }
}
