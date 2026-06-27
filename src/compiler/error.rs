use std::fmt;

use miette::Diagnostic;

/// Compilation errors across the parsing, merging, and validation pipeline.
#[derive(Debug, thiserror::Error)]
pub enum CompileError {
    /// An IO error (file read, canonicalize, etc.).
    Io(#[from] std::io::Error),
    /// One or more parse errors with source spans attached.
    ParseReports(Vec<miette::Report>),
    /// A circular import chain detected during file resolution.
    CircularImport(String),
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
            CompileError::CircularImport(path) => {
                write!(f, "Circular import detected: {}", path)
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
