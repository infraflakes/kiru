//! Where a program's files come from.
//!
//! The compiler is parameterized over the source of its program files: the
//! command line supplies the disk's source, the language server supplies the
//! editor's buffers, and the tests supply a virtual tree. The compiler itself
//! reads no disk; a source answers which files a tree holds and what one
//! file's source is. The tree root is the manifest's, so a source names no
//! target.

use std::path::{Path, PathBuf};

/// The source of a program tree: which files the tree holds, and what one
/// file's source is. Every answer is a plain path or text; the loader owns the
/// namespaces and the parsing.
pub trait ProgramSource {
    /// Every `.kiru` file of the tree under the tree root.
    fn collect(&self, tree_root: &Path) -> std::io::Result<Vec<PathBuf>>;
    /// The source of one program file.
    fn read(&self, path: &Path) -> std::io::Result<String>;
}
