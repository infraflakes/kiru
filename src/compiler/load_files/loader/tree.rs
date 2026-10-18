//! Reading the standard library and the program tree.
//!
//! A file belongs to its directory's namespace: the path from the tree root,
//! with the tree's name as the first segment. The standard library's files come
//! from a table compiled into the binary; the program tree is every `.kiru`
//! file under the entry file's directory, walked recursively. There is no
//! import: a file is in the tree because it is on disk, and resolution is
//! order-free.

use std::path::{Path, PathBuf};

use crate::compiler::load_files::load_error::LoadError;
use crate::compiler::load_files::loaded_program::LoadedProgram;
use crate::compiler::model::Origin;

use super::embedded::EMBEDDED;
use super::record::Loader;

/// The program tree's name: the first namespace segment of every program file.
pub(crate) const PROGRAM_TREE: &str = "root";

/// The standard library's tree name: the first namespace segment of every
/// embedded file.
const STD_TREE: &str = "std";

/// The embedded path prefix the standard library's files are keyed under.
const STD_PREFIX: &str = "<std>/";

/// Load the standard library and the entry file's whole directory tree.
pub(crate) fn load_files(entry: &Path) -> Result<LoadedProgram, LoadError> {
    let entry_path = entry.canonicalize().map_err(|_| LoadError {
        path: entry.to_path_buf(),
        span: crate::syntax::Span::new(0, 0),
        message: format!("cannot find {}", entry.display()),
        source: String::new(),
    })?;
    let tree_root = entry_path.parent().unwrap_or(Path::new(".")).to_path_buf();

    let mut loader = Loader::new();

    for (path, source) in EMBEDDED {
        let namespace = namespace_of(path, STD_PREFIX, STD_TREE);
        loader.add(
            PathBuf::from(path),
            (*source).to_owned(),
            Origin::Embedded,
            namespace,
        )?;
    }

    let mut program_files = Vec::new();
    collect_kiru_files(&tree_root, &mut program_files)?;
    program_files.sort();
    for path in program_files {
        let canonical = path.canonicalize().unwrap_or_else(|_| path.clone());
        let namespace = program_namespace(&tree_root, &canonical);
        let source = std::fs::read_to_string(&canonical).map_err(|error| LoadError {
            path: canonical.clone(),
            span: crate::syntax::Span::new(0, 0),
            message: format!("cannot read {}: {error}", canonical.display()),
            source: String::new(),
        })?;
        let index = loader.add(canonical.clone(), source, Origin::File, namespace)?;
        if canonical == entry_path {
            loader.entry = Some(index);
        }
    }

    let entry = loader.entry.ok_or_else(|| LoadError {
        path: entry_path.clone(),
        span: crate::syntax::Span::new(0, 0),
        message: format!("cannot find {}", entry_path.display()),
        source: String::new(),
    })?;
    Ok(LoadedProgram {
        entry,
        files: loader.files,
        items: loader.items,
        uses: loader.uses,
    })
}

/// The namespace a file's directory names, relative to a tree root: the tree's
/// name, then every directory between the root and the file.
fn namespace_of(path: &str, prefix: &str, tree: &str) -> Vec<String> {
    let relative = path.strip_prefix(prefix).unwrap_or(path);
    let mut segments = vec![tree.to_owned()];
    let mut components: Vec<&str> = relative.split('/').collect();
    components.pop();
    for component in components {
        if !component.is_empty() {
            segments.push(component.to_owned());
        }
    }
    segments
}

/// The namespace of one program file: `root`, then its directories under the
/// tree root.
fn program_namespace(tree_root: &Path, path: &Path) -> Vec<String> {
    let mut segments = vec![PROGRAM_TREE.to_owned()];
    if let Ok(relative) = path.strip_prefix(tree_root) {
        let mut components: Vec<&std::ffi::OsStr> = relative.iter().collect();
        components.pop();
        for component in components {
            segments.push(component.to_string_lossy().into_owned());
        }
    }
    segments
}

/// Every `.kiru` file under `directory`, recursively.
fn collect_kiru_files(directory: &Path, out: &mut Vec<PathBuf>) -> Result<(), LoadError> {
    let entries = std::fs::read_dir(directory).map_err(|error| LoadError {
        path: directory.to_path_buf(),
        span: crate::syntax::Span::new(0, 0),
        message: format!("cannot read {}: {error}", directory.display()),
        source: String::new(),
    })?;
    for entry in entries {
        let entry = entry.map_err(|error| LoadError {
            path: directory.to_path_buf(),
            span: crate::syntax::Span::new(0, 0),
            message: format!("cannot read {}: {error}", directory.display()),
            source: String::new(),
        })?;
        let path = entry.path();
        if path.is_dir() {
            collect_kiru_files(&path, out)?;
        } else if path
            .extension()
            .is_some_and(|extension| extension == "kiru")
        {
            out.push(path);
        }
    }
    Ok(())
}
