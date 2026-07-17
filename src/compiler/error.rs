use std::collections::HashMap;
use std::fmt;
use std::path::Path;

use crate::dsl::Expr;
use crate::error::{SourceFile, spanned_report};

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

/// Spanned error located by an explicit source name and span. Covers the
/// remaining cases where the span is computed separately from any single node
/// (variable-reference resolution, whole-program errors).
pub(crate) fn spanned_err(
    msg: String,
    source: &SourceFile<'_>,
    offset: usize,
    len: usize,
) -> CompileError {
    CompileError::ValidationReport(vec![spanned_report(msg, source, offset, len)])
}

/// Spanned error resolved through the source-text registry by file name. Used
/// when only the declaring file name is known (rather than a `SourceFile`),
/// e.g. whole-program or variable-reference resolution errors.
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

/// Wrap an [`std::io::Error`] into a [`CompileError::Io`] with a descriptive
/// message. Centralizes the repeated `CompileError::Io(std::io::Error::new(..))`
/// construction so callers stay declarative and error wording stays uniform.
pub(crate) fn io_err(context: &str, path: &Path, source: &std::io::Error) -> CompileError {
    CompileError::Io(std::io::Error::new(
        source.kind(),
        format!("{} {}: {}", context, path.display(), source),
    ))
}
