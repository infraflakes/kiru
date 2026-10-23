//! Reading the standard library and a project tree.
//!
//! A file belongs to its directory's namespace: the path from the tree root,
//! with the tree's name as the first segment. The standard library's files come
//! from a table compiled into the binary; a project tree is every `.kiru` file
//! under the manifest's root. There is no import: a file is in the tree because
//! the source names it, and resolution is order-free. Every byte is read
//! through the program source; the loader names no disk.

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

/// Load the standard library and the whole project tree, read through the
/// source the caller names.
pub(crate) fn load_files(
    project: &crate::project::Project,
    program_source: &impl ProgramSource,
) -> Result<LoadedProgram, LoadError> {
    let mut loader = Loader::new();
    loader.add_embedded()?;
    add_tree(&mut loader, &project.root, PROGRAM_TREE, program_source)?;
    Ok(loader.finish())
}

/// Add every file of one tree, each in the namespace its directories name.
fn add_tree(
    loader: &mut Loader,
    root: &Path,
    tree: &str,
    program_source: &impl ProgramSource,
) -> Result<(), LoadError> {
    let mut files = program_source.collect(root).map_err(|error| LoadError {
        path: root.to_path_buf(),
        span: crate::syntax::Span::new(0, 0),
        message: format!("cannot read {}: {error}", root.display()),
        source: String::new(),
    })?;
    files.sort();
    for path in files {
        let Some(relative) = relative_to(root, &path) else {
            return Err(LoadError {
                path: path.clone(),
                span: crate::syntax::Span::new(0, 0),
                message: format!(
                    "`{}` is not under the tree `{}`",
                    path.display(),
                    root.display()
                ),
                source: String::new(),
            });
        };
        let namespace = namespace_of(&relative, tree);
        let source = read_source(program_source, &path)?;
        loader.add(path, source, namespace)?;
    }
    Ok(())
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
