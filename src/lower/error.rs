use miette::Diagnostic;
use thiserror::Error;

#[derive(Debug, Error, Diagnostic)]
pub enum CompileError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("parse error")]
    ParseReports(Vec<miette::Report>),

    #[error("validation error")]
    ValidationReport(Vec<miette::Report>),
}
