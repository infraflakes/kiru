use std::collections::HashMap;

use miette::Diagnostic;

use crate::dsl::{CasePattern, Expr};

/// Identifies the source file a diagnostic span refers to. Bundles the file
/// name and full text so span offsets are always interpreted against the
/// correct source, and so callers pass one value instead of two loose strings
/// (which previously let callers accidentally pass empty strings and produce
/// invalid miette spans that leaked `[Failed to read contents ... OutOfBounds]`
/// into the rendered diagnostic).
///
/// This is the phase-agnostic unit shared by every layer that can emit a
/// spanned diagnostic: the compiler resolve/validate passes and the shell
/// execution helper both build diagnostics from a `SourceFile`, so it lives at
/// the crate root rather than inside `compiler`.
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

/// A miette-based validation error with source span information.
#[derive(Debug, Diagnostic, thiserror::Error)]
#[error("{message}")]
pub(crate) struct SpannedValidationError {
    pub message: String,
    #[label]
    pub span: miette::SourceSpan,
    #[source_code]
    pub source_code: miette::NamedSource<String>,
}

/// Builds a miette diagnostic from a message, source, and span. Centralizes
/// the `SpannedValidationError` construction so every spanned error — whether
/// wrapped in a `CompileError` or pushed into a `Vec<Report>` for batch
/// validation, or emitted by the shell helper — is built identically. The span
/// is clamped to the source text so an out-of-bounds offset can never produce
/// an invalid diagnostic.
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
