//! Where a program's files come from.
//!
//! The compiler is parameterized over the source of its program files: the
//! command line supplies the disk's source, the language server supplies the
//! editor's buffers, and the tests supply a virtual tree. The compiler itself
//! reads no disk; a source answers where a target stands, which files a tree
//! holds, and what one file's source is.

use std::path::{Path, PathBuf};

/// The source of a program tree: where the target stands, which files the tree
/// holds, and what one file's source is. Every answer is a plain path or
/// text; the loader owns the namespaces and the parsing.
pub trait ProgramSource {
    /// The target: a file is the entry the loader records, beside the tree
    /// root the program's namespaces derive from; a directory is the tree
    /// itself, with no entry.
    fn locate(&self, target: &Path) -> std::io::Result<(Option<PathBuf>, PathBuf)>;
    /// Every `.kiru` file of the program tree under the tree root.
    fn collect(&self, tree_root: &Path) -> std::io::Result<Vec<PathBuf>>;
    /// The source of one program file.
    fn read(&self, path: &Path) -> std::io::Result<String>;
}
