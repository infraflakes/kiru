//! The loaded program: its files and the joined item stream.
//!
//! Loading records every file's declarations and `use`s in the namespace its
//! directory names. There is no import and no order to preserve: resolution is
//! order-free, so the stream is just every declaration, each with the file it
//! came from and the namespace it joins.

use std::path::PathBuf;

use crate::model::Origin;
use crate::syntax::{Declaration, Span};

/// One loaded source file, kept for diagnostics.
#[derive(Debug)]
pub(crate) struct LoadedFile {
    pub(crate) path: PathBuf,
    pub(crate) source: String,
    pub(crate) origin: Origin,
}

/// One declaration in the joined stream, with the file it came from and the
/// namespace its directory placed it in.
#[derive(Debug)]
pub(crate) struct ScopedDeclaration {
    pub(crate) file: usize,
    pub(crate) namespace: Vec<String>,
    pub(crate) declaration: Declaration,
}

/// One `use` written in a namespace. What its path ends at decides what it
/// brings into that namespace.
#[derive(Debug)]
pub(crate) struct ScopedUse {
    /// The file that wrote the `use`, for diagnostics.
    pub(crate) file: usize,
    pub(crate) namespace: Vec<String>,
    pub(crate) path: Vec<String>,
    /// The span of the `use` item, for diagnostics.
    pub(crate) span: Span,
}

/// The whole program: every loaded file plus the joined item stream.
#[derive(Debug)]
pub(crate) struct LoadedProgram {
    pub(crate) entry: usize,
    pub(crate) files: Vec<LoadedFile>,
    pub(crate) items: Vec<ScopedDeclaration>,
    pub(crate) uses: Vec<ScopedUse>,
}
