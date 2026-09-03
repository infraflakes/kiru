use crate::diagnostics::Diagnostic;
use thiserror::Error;

/// Everything that can fail while lowering a `.kiru` config: an I/O
/// failure, or one or more source diagnostics (parse and validation errors
/// share the same shape once rendered).
#[derive(Debug, Error)]
pub(crate) enum CompileError {
    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("compile error")]
    Diagnostics(Vec<Diagnostic>),
}

impl CompileError {
    /// Wrap a single diagnostic as a compile error. Most failure sites
    /// emit exactly one diagnostic; batches are the exception.
    pub(crate) fn diagnostic(diag: Diagnostic) -> Self {
        Self::Diagnostics(vec![diag])
    }
}
