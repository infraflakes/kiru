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
use crate::compiler::sources::ProgramSource;

use super::record::Loader;

/// The program tree's name: the first namespace segment of every program file.
pub(crate) const PROGRAM_TREE: &str = "root";

/// The standard library's tree name: the first namespace segment of every
/// embedded file.
pub(crate) const STD_TREE: &str = "std";

/// Load the standard library and the entry file's whole program tree, read
/// through the source the caller names.
pub(crate) fn load_files(
    target: &Path,
    program_source: &impl ProgramSource,
) -> Result<LoadedProgram, LoadError> {
    let (entry_path, tree_root) = program_source.locate(target).map_err(|_| LoadError {
        path: target.to_path_buf(),
        span: crate::syntax::Span::new(0, 0),
        message: format!("cannot find {}", target.display()),
        source: String::new(),
    })?;

    let mut loader = Loader::new();
    loader.add_embedded()?;

    let mut program_files = program_source
        .collect(&tree_root)
        .map_err(|error| LoadError {
            path: tree_root.clone(),
            span: crate::syntax::Span::new(0, 0),
            message: format!("cannot read {}: {error}", tree_root.display()),
            source: String::new(),
        })?;
    program_files.sort();
    for path in program_files {
        let canonical = path.canonicalize().unwrap_or_else(|_| path.clone());
        let namespace = program_namespace(&tree_root, &canonical);
        let source = program_source.read(&canonical).map_err(|error| LoadError {
            path: canonical.clone(),
            span: crate::syntax::Span::new(0, 0),
            message: format!("cannot read {}: {error}", canonical.display()),
            source: String::new(),
        })?;
        let index = loader.add(canonical.clone(), source, namespace)?;
        if entry_path.as_deref() == Some(canonical.as_path()) {
            loader.entry = Some(index);
        }
    }

    // A named entry must be among the tree's files; a directory target names
    // the tree and no entry.
    if let Some(entry_path) = &entry_path
        && loader.entry.is_none()
    {
        return Err(LoadError {
            path: entry_path.clone(),
            span: crate::syntax::Span::new(0, 0),
            message: format!("cannot find {}", entry_path.display()),
            source: String::new(),
        });
    }
    Ok(LoadedProgram {
        entry: loader.entry,
        files: loader.files,
        items: loader.items,
        uses: loader.uses,
    })
}

/// Load one file by itself: the library it belongs to and this file, with no
/// entry. A program tree is not read: a file is checked in its own context.
/// A file under the library root is checked against the library read from its
/// own directory, with the edited source in place of its file; any other file
/// is checked with the embedded library mounted.
pub(crate) fn load_single(path: &Path, source: &str) -> Result<LoadedProgram, LoadError> {
    let mut loader = Loader::new();
    match library_file(path) {
        Some((relative, root)) => {
            let mut files = collect_kiru_files(&root).map_err(|error| LoadError {
                path: root.clone(),
                span: crate::syntax::Span::new(0, 0),
                message: format!("cannot read {}: {error}", root.display()),
                source: String::new(),
            })?;
            files.sort();
            for file in files {
                let Some(under_library) = relative_to(&root, &file) else {
                    continue;
                };
                let namespace = namespace_of(&under_library, STD_TREE);
                if under_library == relative {
                    loader.add(path.to_path_buf(), source.to_owned(), namespace)?;
                } else {
                    let text = std::fs::read_to_string(&file).map_err(|error| LoadError {
                        path: file.clone(),
                        span: crate::syntax::Span::new(0, 0),
                        message: format!("cannot read {}: {error}", file.display()),
                        source: String::new(),
                    })?;
                    loader.add(file, text, namespace)?;
                }
            }
        }
        None => {
            loader.add_embedded()?;
            loader.add(
                path.to_path_buf(),
                source.to_owned(),
                vec![PROGRAM_TREE.to_owned()],
            )?;
        }
    }
    Ok(LoadedProgram {
        entry: None,
        files: loader.files,
        items: loader.items,
        uses: loader.uses,
    })
}

/// The path of `file` relative to `root`, in the slash form namespaces use.
fn relative_to(root: &Path, file: &Path) -> Option<String> {
    let relative = file.strip_prefix(root).ok()?;
    Some(relative.to_string_lossy().replace('\\', "/"))
}

/// The library-relative path a file under the library root holds, when it
/// holds one: the library root is the `std` directory the language declares.
fn library_file(path: &Path) -> Option<(String, PathBuf)> {
    for ancestor in path.ancestors() {
        if ancestor.file_name().is_some_and(|name| name == STD_TREE) {
            let relative = relative_to(ancestor, path)?;
            return Some((relative, ancestor.to_path_buf()));
        }
    }
    None
}

/// The namespace a file's directory names, relative to a tree root: the tree's
/// name, then every directory between the root and the file.
pub(super) fn namespace_of(path: &str, tree: &str) -> Vec<String> {
    let mut segments = vec![tree.to_owned()];
    let mut components: Vec<&str> = path.split('/').collect();
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

/// Every `.kiru` file under `directory`, recursively. The disk source's
/// program tree is this walk; the loader reads what it names.
pub(in crate::compiler) fn collect_kiru_files(directory: &Path) -> std::io::Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    for entry in std::fs::read_dir(directory)? {
        let path = entry?.path();
        if path.is_dir() {
            files.extend(collect_kiru_files(&path)?);
        } else if path
            .extension()
            .is_some_and(|extension| extension == "kiru")
        {
            files.push(path);
        }
    }
    Ok(files)
}
