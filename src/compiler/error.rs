use std::collections::HashMap;
use std::fmt;
use std::path::Path;

use miette::Diagnostic;

use crate::dsl::{CasePattern, Expr};

/// Identifies the source file a diagnostic span refers to. Bundles the file
/// name and full text so span offsets are always interpreted against the
/// correct source, and so callers pass one value instead of two loose strings
/// (which previously let callers accidentally pass empty strings and produce
/// invalid miette spans that leaked `[Failed to read contents ... OutOfBounds]`
/// into the rendered diagnostic).
pub(crate) struct SourceFile<'a> {
    pub name: &'a str,
    pub text: &'a str,
}

impl<'a> SourceFile<'a> {
    /// Builds a `SourceFile` from a node's source name, looking the text up in
    /// the source-text registry. This is how every diagnostic resolves the
    /// correct file when a project's body is merged from several `.kiru` files:
    /// the span offset always comes from the file that actually defined the
    /// node, not from the first file that happened to declare `pr <name>`.
    ///
    /// Falls back to empty text when the name is unknown, so the span is still
    /// clamped (never out of bounds) instead of panicking.
    pub(crate) fn from_registry<'b>(sources: &'a HashMap<String, String>, name: &'b str) -> Self
    where
        'b: 'a,
    {
        Self {
            name,
            text: sources.get(name).map(|s| s.as_str()).unwrap_or(""),
        }
    }
}

/// Compilation errors across the parsing, merging, and validation pipeline.
#[derive(Debug, thiserror::Error)]
pub enum CompileError {
    /// An IO error (file read, canonicalize, etc.).
    Io(#[from] std::io::Error),
    /// One or more parse errors with source spans attached.
    ParseReports(Vec<miette::Report>),
    /// Multiple validation errors; each original diagnostic is kept so its
    /// source, labels, and spans survive rendering.
    ValidationReport(Vec<miette::Report>),
}

impl fmt::Display for CompileError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CompileError::Io(e) => write!(f, "IO error: {}", e),
            CompileError::ParseReports(reports) => {
                for report in reports {
                    writeln!(f, "{}", report)?;
                }
                Ok(())
            }
            CompileError::ValidationReport(reports) => {
                for report in reports {
                    writeln!(f, "{}", report)?;
                }
                Ok(())
            }
        }
    }
}

/// A miette-based validation error with source span information.
#[derive(Debug, Diagnostic, thiserror::Error)]
#[error("{message}")]
pub struct SpannedValidationError {
    pub message: String,
    #[label]
    pub span: miette::SourceSpan,
    #[source_code]
    pub source_code: miette::NamedSource<String>,
}

pub(crate) fn spanned_err(
    msg: String,
    source: &SourceFile<'_>,
    offset: usize,
    len: usize,
) -> CompileError {
    CompileError::ValidationReport(vec![spanned_report(msg, source, offset, len)])
}

/// Spanned error located by an explicit source name and span. Covers the
/// remaining cases where the span is computed separately from any single node
/// (variable-reference resolution, whole-program errors).
pub(crate) fn spanned_err_named(
    msg: impl Into<String>,
    sources: &HashMap<String, String>,
    name: &str,
    offset: usize,
    len: usize,
) -> CompileError {
    spanned_err(
        msg.into(),
        &SourceFile::from_registry(sources, name),
        offset,
        len,
    )
}

/// Spanned error for an optional `Expr` field. When the field is present the
/// span references the file that defined it; when absent it falls back to
/// `fallback_name` (the first merged declaration's file) with a zero-length
/// span. Centralizes the "use the defining file, not the first merged
/// declaration" rule so it can't be re-introduced per field.
pub(crate) fn spanned_err_on_field(
    msg: impl Into<String>,
    sources: &HashMap<String, String>,
    field: &Option<Expr>,
    fallback_name: &str,
) -> CompileError {
    let name = field
        .as_ref()
        .map(|e| e.source_name())
        .unwrap_or(fallback_name);
    let (offset, len) = field.as_ref().map(|e| e.offset_len()).unwrap_or((0, 1));
    spanned_err_named(msg, sources, name, offset, len)
}

/// Builds a miette diagnostic from a message, source, and span. Centralizes
/// the `SpannedValidationError` construction so every spanned error — whether
/// wrapped in a `CompileError` or pushed into a `Vec<Report>` for batch
/// validation — is built identically. The span is clamped to the source text
/// so an out-of-bounds offset can never produce an invalid diagnostic.
pub(crate) fn spanned_report(
    msg: String,
    source: &SourceFile<'_>,
    offset: usize,
    len: usize,
) -> miette::Report {
    miette::Report::new(SpannedValidationError {
        message: msg,
        span: clamped_span(source.text, offset, len),
        source_code: miette::NamedSource::new(source.name, source.text.to_string()),
    })
}

/// A parsed node that can locate itself in a source file: it knows the file
/// that defined it and the byte span it occupies. Implementing this lets a
/// diagnostic be built from the node alone instead of re-deriving the source
/// name and span at every call site (which previously let callers accidentally
/// fall back to the first merged declaration's file).
pub(crate) trait Spanned {
    fn source_name(&self) -> &str;
    fn offset_len(&self) -> (usize, usize);
}

impl Spanned for Expr {
    fn source_name(&self) -> &str {
        self.source_name()
    }
    fn offset_len(&self) -> (usize, usize) {
        self.offset_len()
    }
}

impl Spanned for CasePattern {
    fn source_name(&self) -> &str {
        self.source_name()
    }
    fn offset_len(&self) -> (usize, usize) {
        self.offset_len()
    }
}

/// Spanned miette report located on a node, resolved against the source-text
/// registry. Used by the batch validation pass, which collects `Report`s.
pub(crate) fn spanned_report_on<S: Spanned + ?Sized>(
    msg: impl Into<String>,
    sources: &HashMap<String, String>,
    node: &S,
) -> miette::Report {
    let (offset, len) = node.offset_len();
    spanned_report(
        msg.into(),
        &SourceFile::from_registry(sources, node.source_name()),
        offset,
        len,
    )
}

/// Returns a miette span guaranteed to sit within `text`. Resolves the root
/// cause of the leaked `[Failed to read contents for label <none> ...
/// OutOfBounds]` artifact: a span whose offset ran past the end of (an empty)
/// source. The graphical handler reads the spanned bytes, so clamping keeps
/// that read in bounds regardless of what a caller passed.
fn clamped_span(text: &str, offset: usize, len: usize) -> miette::SourceSpan {
    let text_len = text.len();
    if text_len == 0 {
        return miette::SourceSpan::new(miette::SourceOffset::from(0), 0);
    }
    let safe_offset = offset.min(text_len - 1);
    let available = text_len - safe_offset;
    let safe_len = len.max(1).min(available);
    miette::SourceSpan::new(safe_offset.into(), safe_len)
}

/// Wrap an [`std::io::Error`] into a [`CompileError::Io`] with a descriptive
/// message. Centralizes the repeated `CompileError::Io(std::io::Error::new(..))`
/// construction so callers stay declarative and error wording stays uniform.
pub(crate) fn io_err(context: &str, path: &Path, source: &std::io::Error) -> CompileError {
    CompileError::Io(std::io::Error::new(
        source.kind(),
        format!("{} {}: {}", context, path.display(), source),
    ))
}
