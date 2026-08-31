use crate::diagnostics::Diagnostic;
use thiserror::Error;

#[derive(Debug, Error)]
pub(crate) enum CompileError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("parse error")]
    Parse(Vec<Diagnostic>),

    #[error("validation error")]
    Validation(Vec<Diagnostic>),
}
