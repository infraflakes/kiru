use miette::Diagnostic;
use std::fmt;

/// Errors that can occur during function execution.
#[derive(Debug, Diagnostic, thiserror::Error)]
pub(crate) enum RuntimeError {
    Lookup(String),
    Io(#[from] std::io::Error),
    Exec { cmd: String, detail: String },
}

impl RuntimeError {
    /// Create an `Exec` error from a failed or non-zero exit command.
    ///
    /// `detail` already carries the human-readable cause (e.g. via
    /// `subprocess::describe_exit_failure`), so the caller supplies it directly.
    pub(crate) fn exec_io_error(cmd: impl ToString, detail: impl ToString) -> Self {
        RuntimeError::Exec {
            cmd: cmd.to_string(),
            detail: detail.to_string(),
        }
    }
}

impl fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RuntimeError::Lookup(s) => write!(f, "lookup error: {}", s),
            RuntimeError::Io(e) => write!(f, "IO error: {}", e),
            RuntimeError::Exec { cmd, detail } => {
                write!(f, "execution failed: {}: {}", cmd, detail)
            }
        }
    }
}
