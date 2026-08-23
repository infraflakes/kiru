use std::collections::HashMap;

use miette::Diagnostic;
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
    pub(crate) fn from_registry(sources: &'a HashMap<String, String>, name: &'a str) -> Self {
        Self {
            name,
            text: sources.get(name).map(|s| s.as_str()).unwrap_or(""),
        }
    }
}

/// A source position for a spanned diagnostic: the declaring file name, the
/// byte span within it, and the registry of file texts used to resolve a
/// [`SourceFile`].
///
/// Replaces the four loose `(sources, source_name, offset, len)` values that
/// every spanned-error helper otherwise took as separate arguments — bundling
/// them removes the `clippy::too_many_arguments` suppressions that used to
/// litter the compiler and keeps call sites passing one value.
pub(crate) struct Span<'a> {
    pub source_name: &'a str,
    pub offset: usize,
    pub len: usize,
    pub sources: &'a HashMap<String, String>,
}

impl<'a> Span<'a> {
    /// Resolve the registry entry for this span's file into a [`SourceFile`].
    pub(crate) fn source_file(&self) -> SourceFile<'a> {
        SourceFile::from_registry(self.sources, self.source_name)
    }

    /// Build a miette report at this span with `msg` as its message.
    pub(crate) fn report(&self, msg: impl Into<String>) -> miette::Report {
        spanned_report(msg.into(), &self.source_file(), self.offset, self.len)
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

/// Spanned miette report resolved against the source-text registry from a
/// node's own source name and span. Used by the batch validation pass, which
/// collects `Report`s.
pub(crate) fn spanned_report_on(
    msg: impl Into<String>,
    sources: &HashMap<String, String>,
    source_name: &str,
    offset: usize,
    len: usize,
) -> miette::Report {
    spanned_report(
        msg.into(),
        &SourceFile::from_registry(sources, source_name),
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

/// Render a miette diagnostic to stderr using the installed handler.
///
/// Centralizes diagnostic printing so callers do not reach for ad-hoc
/// `eprintln!("{:?}", report)`, which drops the handler's source snippets and
/// styling. The handler is installed once in `main` via `miette::set_hook`.
///
/// Lives at the crate root next to the other miette plumbing because both the
/// compiler (skip warnings during metadata-only parsing) and the CLI (batch
/// error reports) emit diagnostics: a single printer keeps every layer
/// rendering through the same handler.
pub(crate) fn print_diagnostic(report: &miette::Report) {
    use std::io::Write;

    let mut stderr = std::io::stderr();
    if writeln!(stderr, "{:?}", report).is_err() {
        std::eprintln!("{:?}", report);
    }
}
