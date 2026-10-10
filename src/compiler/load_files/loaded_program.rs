//! The loaded program: its files and the joined item stream.
//!
//! Loading expands every `import` in place, depth first, so the items of a
//! program are one ordered sequence: each file's items in order, with an
//! imported file's items spliced where the import sits. A file is loaded once;
//! a second import of it, or an import cycle, is an error.

use std::path::PathBuf;

use crate::model::Origin;
use crate::syntax::Declaration;

/// One loaded source file, kept for diagnostics.
#[derive(Debug)]
pub(crate) struct LoadedFile {
    pub(crate) path: PathBuf,
    pub(crate) source: String,
    pub(crate) origin: Origin,
}

/// One declaration in the joined stream, with the file it came from and the
/// namespace a `mod` block placed it in.
#[derive(Debug)]
pub(crate) struct ScopedDeclaration {
    pub(crate) file: usize,
    pub(crate) namespace: Vec<String>,
    pub(crate) declaration: Declaration,
}

/// The whole program: every loaded file plus the joined item stream.
#[derive(Debug)]
pub(crate) struct LoadedProgram {
    pub(crate) entry: usize,
    pub(crate) files: Vec<LoadedFile>,
    pub(crate) items: Vec<ScopedDeclaration>,
}
