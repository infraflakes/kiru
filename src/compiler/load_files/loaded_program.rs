//! The loaded program: its files and the joined item stream.
//!
//! Loading records every file's declarations and `use`s in the namespace its
//! directory names. There is no import and no order to preserve: resolution is
//! order-free, so the stream is just every declaration, each with the file it
//! came from and the namespace it joins.

use std::path::PathBuf;

use crate::compiler::Diagnostic;
use crate::syntax::{Declaration, Span, TypeExpr};

/// One loaded source file, kept for diagnostics.
#[derive(Debug)]
pub(crate) struct LoadedFile {
    pub(crate) path: PathBuf,
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
    pub(crate) entry: Option<usize>,
    pub(crate) files: Vec<LoadedFile>,
    pub(crate) items: Vec<ScopedDeclaration>,
    pub(crate) uses: Vec<ScopedUse>,
    /// The syntax errors recovery found, positioned in the files they came
    /// from. A lexer error fails the load instead of filling this list.
    pub(crate) syntax: Vec<Diagnostic>,
}
