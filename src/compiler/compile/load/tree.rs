//! Reading the standard library and the program tree.
//!
//! A file belongs to its directory's namespace: the path from the tree root,
//! with the tree's name as the first segment. The standard library's files come
//! from a table compiled into the binary; a program tree is every `.kiru` file
//! under the target's directory. There is no import: a file is in the tree
//! because the source names it, and resolution is order-free. Every byte is
//! read through the program source; the loader names no disk.

use std::path::Path;

use super::sources::ProgramSource;
use crate::compiler::compile::load::load_error::LoadError;
use crate::compiler::compile::load::loaded_program::LoadedProgram;

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
        let namespace = program_namespace(&tree_root, &path);
        let source = read_source(program_source, &path)?;
        let index = loader.add(path.clone(), source, namespace)?;
        if entry_path.as_deref() == Some(path.as_path()) {
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
        syntax: loader.syntax,
    })
}

/// Load the files of one file's context: this file beside the library it
/// belongs to, with no entry and no program tree.
///
/// With no library root, the embedded table is mounted and the analyzed file
/// stands in the root namespace. With a library root, the embedded table is
/// not mounted: the source collects the root's files, each read through it, and
/// every file joins the `std` tree under its directories. The analyzed file is
/// one of them, and the source answers its buffer for it.
pub(crate) fn load_single(
    path: &Path,
    program_source: &impl ProgramSource,
    library: Option<&Path>,
) -> Result<LoadedProgram, LoadError> {
    let mut loader = Loader::new();
    match library {
        None => {
            loader.add_embedded()?;
            let source = read_source(program_source, path)?;
            loader.add(path.to_path_buf(), source, vec![PROGRAM_TREE.to_owned()])?;
        }
        Some(root) => {
            let mut files = program_source.collect(root).map_err(|error| LoadError {
                path: root.to_path_buf(),
                span: crate::syntax::Span::new(0, 0),
                message: format!("cannot read {}: {error}", root.display()),
                source: String::new(),
            })?;
            files.sort();
            for file in files {
                let Some(under_library) = relative_to(root, &file) else {
                    continue;
                };
                let namespace = namespace_of(&under_library, STD_TREE);
                let source = read_source(program_source, &file)?;
                loader.add(file, source, namespace)?;
            }
        }
    }
    Ok(LoadedProgram {
        entry: None,
        files: loader.files,
        items: loader.items,
        uses: loader.uses,
        syntax: loader.syntax,
    })
}

/// Read one file through the source, naming it when the source cannot.
fn read_source(program_source: &impl ProgramSource, path: &Path) -> Result<String, LoadError> {
    program_source.read(path).map_err(|error| LoadError {
        path: path.to_path_buf(),
        span: crate::syntax::Span::new(0, 0),
        message: format!("cannot read {}: {error}", path.display()),
        source: String::new(),
    })
}

/// The path of `file` relative to `root`, in the slash form namespaces use.
fn relative_to(root: &Path, file: &Path) -> Option<String> {
    let relative = file.strip_prefix(root).ok()?;
    Some(relative.to_string_lossy().replace('\\', "/"))
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
