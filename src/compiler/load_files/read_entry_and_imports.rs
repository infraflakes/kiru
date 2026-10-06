//! Reading the entry file and every file it imports.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use crate::model::Origin;
use crate::syntax;
use crate::syntax::Span;

use super::load_error::LoadError;
use super::loaded_program::{LoadedFile, LoadedProgram};
use super::resolve_import_paths::resolve_import;

/// The standard library, compiled into every program and always visible.
///
/// These files are seeded before the entry so their declarations come first.
pub(crate) const EMBEDDED: &[(&str, &str)] = &[
    ("<std>/text.kiru", include_str!("../../../stdlib/text.kiru")),
    (
        "<std>/lists.kiru",
        include_str!("../../../stdlib/lists.kiru"),
    ),
    ("<std>/env.kiru", include_str!("../../../stdlib/env.kiru")),
    ("<std>/path.kiru", include_str!("../../../stdlib/path.kiru")),
    ("<std>/io.kiru", include_str!("../../../stdlib/io.kiru")),
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
        loaded: HashMap::new(),
        loading: HashSet::new(),
    };
    loader.seed_embedded()?;
    let entry_index = loader.load_file(&entry_path, Span::new(0, 0), entry)?;
    Ok(LoadedProgram {
        entry: entry_index,
        files: loader.files,
    })
}

struct Loader {
    files: Vec<LoadedFile>,
    loaded: HashMap<PathBuf, usize>,
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
            self.files.push(LoadedFile {
                path: PathBuf::from(path),
                source: (*source).to_owned(),
                file,
                imports: Vec::new(),
                origin: Origin::Embedded,
            });
        }
        Ok(())
    }

    fn load_file(
        &mut self,
        canonical: &Path,
        span: Span,
        reported: &Path,
    ) -> Result<usize, LoadError> {
        if let Some(&index) = self.loaded.get(canonical) {
            return Ok(index);
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
        self.loading.insert(canonical.to_path_buf());
        let mut imports = Vec::new();
        for import in &file.imports {
            let Some(target) = resolve_import(&directory, &import.path) else {
                return Err(LoadError {
                    path: canonical.to_path_buf(),
                    span: import.span,
                    message: format!("cannot find import {}", import.path),
                    source,
                });
            };
            let loaded = match self.load_file(&target, import.span, canonical) {
                Ok(loaded) => loaded,
                Err(error) if error.source.is_empty() => {
                    return Err(LoadError {
                        path: error.path,
                        span: error.span,
                        message: error.message,
                        source,
                    });
                }
                Err(error) => return Err(error),
            };
            imports.push(loaded);
        }
        self.loading.remove(canonical);

        let index = self.files.len();
        self.files.push(LoadedFile {
            path: canonical.to_path_buf(),
            source,
            file,
            imports,
            origin: Origin::File,
        });
        self.loaded.insert(canonical.to_path_buf(), index);
        Ok(index)
    }
}
