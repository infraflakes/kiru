//! Reading the entry file and every file it imports.
//!
//! Loading joins every file into one ordered item stream: a file's items in
//! order, with each `import` replaced by the imported file's items. A file is
//! loaded once; importing it a second time, or importing in a cycle, is an
//! error. The standard library is loaded by the same rule, from text compiled
//! into the binary rather than from disk, so its own `import`s state its shape.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::model::Origin;
use crate::syntax;
use crate::syntax::{Item, Module, Span};

use super::load_error::LoadError;
use super::loaded_program::{LoadedFile, LoadedProgram, ScopedDeclaration, ScopedUse};
use super::resolve_import_paths::resolve_import;

/// The standard library's files, compiled into every program.
///
/// This is a content table, not a graph: it says which files exist and what is
/// in them. The order and the dependencies come from the `import`s inside the
/// files, exactly as they do for a program's own files. `<std>/std.kiru` is
/// the root and every other file is reached from it.
pub(crate) const EMBEDDED: &[(&str, &str)] = &[
    ("<std>/std.kiru", include_str!("../../../stdlib/std.kiru")),
    ("<std>/text.kiru", include_str!("../../../stdlib/text.kiru")),
    ("<std>/vec.kiru", include_str!("../../../stdlib/vec.kiru")),
    ("<std>/map.kiru", include_str!("../../../stdlib/map.kiru")),
    ("<std>/set.kiru", include_str!("../../../stdlib/set.kiru")),
    ("<std>/env.kiru", include_str!("../../../stdlib/env.kiru")),
    ("<std>/fs.kiru", include_str!("../../../stdlib/fs.kiru")),
    ("<std>/path.kiru", include_str!("../../../stdlib/path.kiru")),
    ("<std>/time.kiru", include_str!("../../../stdlib/time.kiru")),
    (
        "<std>/process.kiru",
        include_str!("../../../stdlib/process.kiru"),
    ),
    ("<std>/cli.kiru", include_str!("../../../stdlib/cli.kiru")),
];

/// The standard library's root: the one embedded file seeded before the entry.
const STD_ROOT: &str = "<std>/std.kiru";

/// The text of an embedded file, when `path` names one.
fn embedded_source(path: &Path) -> Option<&'static str> {
    let path = path.to_str()?;
    EMBEDDED
        .iter()
        .find(|(name, _)| *name == path)
        .map(|(_, source)| *source)
}

/// Resolve an import inside an embedded file: joined to the importing file's
/// directory, and accepted only when the embed table knows the result, which
/// also rules out climbing out of the standard library.
fn embedded_import(directory: &Path, import_path: &str) -> Option<PathBuf> {
    let path = Path::new(import_path);
    let candidate = if path.is_absolute() {
        path.to_path_buf()
    } else {
        directory.join(path)
    };
    embedded_source(&candidate).map(|_| candidate)
}

/// Load the standard library and the entry file, and everything they import.
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
        uses: Vec::new(),
        loaded: HashMap::new(),
        loading: HashSet::new(),
    };
    let root = PathBuf::from(STD_ROOT);
    loader.load(&root, Span::new(0, 0), &root)?;
    let entry_index = loader.load(&entry_path, Span::new(0, 0), entry)?;
    Ok(LoadedProgram {
        entry: entry_index,
        files: loader.files,
        items: loader.items,
        uses: loader.uses,
    })
}

struct Loader {
    files: Vec<LoadedFile>,
    items: Vec<ScopedDeclaration>,
    uses: Vec<ScopedUse>,
    /// The index of every file already loaded, for the duplicate-import check.
    loaded: HashMap<PathBuf, usize>,
    /// The files being loaded, for the cycle check.
    loading: HashSet<PathBuf>,
}

impl Loader {
    /// Load one file and splice its items into the stream, replacing each
    /// `import` with the imported file's items in place. The text comes from
    /// the embed table when the path names a standard library file, and from
    /// the filesystem otherwise.
    fn load(&mut self, path: &Path, span: Span, reported: &Path) -> Result<usize, LoadError> {
        if self.loaded.contains_key(path) {
            return Err(load_error(
                reported,
                span,
                format!("`{}` is imported more than once", path.display()),
            ));
        }
        if self.loading.contains(path) {
            return Err(load_error(
                reported,
                span,
                format!("import cycle through {}", path.display()),
            ));
        }

        let embedded = embedded_source(path);
        let (source, origin) = match embedded {
            Some(source) => (source.to_owned(), Origin::Embedded),
            None => {
                let source = std::fs::read_to_string(path).map_err(|error| {
                    load_error(
                        reported,
                        span,
                        format!("cannot read {}: {error}", path.display()),
                    )
                })?;
                (source, Origin::File)
            }
        };
        let file = match syntax::parse_file(&source) {
            Ok(file) => file,
            Err(error) => {
                return Err(LoadError {
                    path: path.to_path_buf(),
                    span: error.span,
                    message: error.message,
                    source,
                });
            }
        };

        let directory = path.parent().unwrap_or(Path::new(".")).to_path_buf();
        let index = self.files.len();
        self.files.push(LoadedFile {
            path: path.to_path_buf(),
            source,
            origin,
        });
        self.loading.insert(path.to_path_buf());
        for item in file.items {
            match item {
                Item::Import(import) => {
                    let target = match embedded {
                        Some(_) => embedded_import(&directory, &import.path),
                        None => resolve_import(&directory, &import.path),
                    };
                    let Some(target) = target else {
                        return Err(load_error(
                            path,
                            import.span,
                            format!("cannot find import {}", import.path),
                        ));
                    };
                    match self.load(&target, import.span, path) {
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
        self.loading.remove(path);
        self.loaded.insert(path.to_path_buf(), index);
        Ok(index)
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
            Item::Use(use_item) => self.uses.push(ScopedUse {
                file,
                declarations_before: self.items.len(),
                namespace: Vec::new(),
                path: use_item.path,
                span: use_item.span,
            }),
            // Imports are expanded by `load`, which never routes one here.
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
        for use_item in module.uses {
            self.uses.push(ScopedUse {
                file,
                declarations_before: self.items.len(),
                namespace: module.path.clone(),
                path: use_item.path,
                span: use_item.span,
            });
        }
    }
}

/// A load failure with no source text attached; the caller fills it in from
/// the file it is reading, so the diagnostic points at the right file.
fn load_error(path: &Path, span: Span, message: String) -> LoadError {
    LoadError {
        path: path.to_path_buf(),
        span,
        message,
        source: String::new(),
    }
}
