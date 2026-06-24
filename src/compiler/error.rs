use std::fmt;

use miette::Diagnostic;

#[derive(Debug, thiserror::Error)]
pub enum CompileError {
    Io(#[from] std::io::Error),
    ParseReports(Vec<miette::Report>),
    CircularImport(String),
    Validation(String),
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
            CompileError::Validation(msg) => write!(f, "Validation error: {}", msg),
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
