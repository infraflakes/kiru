use std::collections::HashMap;
use std::fmt;
use std::path::Path;

use miette::Diagnostic;

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
    /// A validation error with source span information.
    ValidationReport(miette::Report),
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
            CompileError::ValidationReport(report) => write!(f, "{}", report),
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
    CompileError::ValidationReport(spanned_report(msg, source, offset, len))
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
