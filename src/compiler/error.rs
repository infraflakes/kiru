use std::fmt;
use std::path::Path;

use crate::error::{Span, spanned_report};

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

/// Spanned [`CompileError`] built from a [`Span`]. Centralizes the registry
/// lookup so the `(sources, source_name, offset, len)` tuple is never passed
/// loose through the compiler.
pub(crate) fn spanned_err(span: &Span, msg: impl Into<String>) -> CompileError {
    CompileError::ValidationReport(vec![spanned_report(
        msg.into(),
        &span.source_file(),
        span.offset,
        span.len,
    )])
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
