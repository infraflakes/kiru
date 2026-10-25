//! The loaded program: its files and the joined item stream.
//!
//! Loading records every file's declarations and `use`s in the namespace its
//! directory names. There is no import and no order to preserve: resolution is
//! order-free, so the stream is just every declaration, each with the file it
//! came from and the namespace it joins.

use std::path::{Path, PathBuf};

use crate::compiler::Diagnostic;
use crate::compiler::graph::{DeclarationKind, FileOrigin};
use crate::syntax::{Declaration, Span, TypeExpr};

/// One loaded source file, kept for diagnostics.
#[derive(Debug)]
pub(crate) struct LoadedFile {
    pub(crate) path: PathBuf,
    pub(crate) origin: FileOrigin,
    pub(crate) source: String,
    /// The namespace the file's directory names, as the loader derived it.
    pub(crate) namespace: Vec<String>,
}

/// One declaration in the joined stream, with the file it came from and the
/// namespace its directory placed it in.
#[derive(Debug)]
pub(crate) struct ScopedDeclaration {
    pub(crate) file: usize,
    pub(crate) namespace: Vec<String>,
    /// The model kind the declaration form carries. The loader derives it once
    /// from the surface form, and every later pass reads this fact.
    pub(crate) kind: DeclarationKind,
    pub(crate) declaration: Declaration,
    /// The type an `impl` block files this function under, when it came from
    /// one. The block is flattened into its functions when loaded, so every
    /// function carries its block's target.
    pub(crate) impl_target: Option<ImplTarget>,
}

/// The type an `impl` block files its functions under, written as a type
/// expression so every path form resolves the one way a written type does.
#[derive(Debug)]
pub(crate) struct ImplTarget {
    pub(crate) ty: TypeExpr,
}

/// One `use` written in a namespace. What its path ends at decides what it
/// brings into that namespace.
#[derive(Debug)]
pub(crate) struct ScopedUse {
    /// The file that wrote the `use`, for diagnostics.
    pub(crate) file: usize,
    pub(crate) namespace: Vec<String>,
    pub(crate) path: Vec<String>,
    /// The span the path covers.
    pub(crate) path_span: Span,
    /// The span of the name the path delivers.
    pub(crate) delivered: Span,
    /// The span of the `use` item, for diagnostics.
    pub(crate) span: Span,
}

/// The whole program: every loaded file plus the joined item stream.
#[derive(Debug)]
pub(crate) struct LoadedProgram {
    pub(crate) files: Vec<LoadedFile>,
    pub(crate) items: Vec<ScopedDeclaration>,
    pub(crate) uses: Vec<ScopedUse>,
    /// The syntax errors recovery found, positioned in the files they came
    /// from. A lexer error fails the load instead of filling this list.
    pub(crate) syntax: Vec<Diagnostic>,
}

impl LoadedProgram {
    /// Whether a path belongs to the program tree. A path the loaded files do
    /// not name is a project fact, so it is treated as the program's.
    pub(crate) fn is_program_path(&self, path: &Path) -> bool {
        self.files
            .iter()
            .find(|file| file.path == path)
            .is_none_or(|file| file.origin == FileOrigin::Program)
    }
}
