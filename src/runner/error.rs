use miette::Diagnostic;
use std::fmt;

/// Errors that can occur during function execution.
#[derive(Debug, Diagnostic, thiserror::Error)]
pub(crate) enum RuntimeError {
    Lookup(String),
    Io(#[from] std::io::Error),
    Exec {
        cmd: String,
        exit_code: Option<i32>,
        detail: String,
    },
    Panic(String),
}

impl RuntimeError {
    /// Create an `Exec` error from an I/O failure.
    pub(crate) fn exec_io_error(cmd: impl ToString, err: impl ToString) -> Self {
        RuntimeError::Exec {
            cmd: cmd.to_string(),
            exit_code: None,
            detail: err.to_string(),
        }
    }

    /// Create an `Exec` error from a non-zero exit code.
    pub(crate) fn exec_exit_code(cmd: impl ToString, code: Option<i32>) -> Self {
        RuntimeError::Exec {
            cmd: cmd.to_string(),
            exit_code: code,
            detail: String::new(),
        }
    }
}

impl fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            RuntimeError::Lookup(s) => write!(f, "lookup error: {}", s),
            RuntimeError::Io(e) => write!(f, "IO error: {}", e),
            RuntimeError::Exec {
                cmd,
                exit_code,
                detail,
            } => match exit_code {
                None => write!(f, "execution failed: {}: {}", cmd, detail),
                Some(code) if detail.is_empty() => {
                    write!(f, "execution failed: {} with exit code {}", cmd, code)
                }
                Some(code) => {
                    write!(
                        f,
                        "execution failed: {}: {} (exit code {})",
                        cmd, detail, code
                    )
                }
            },
            RuntimeError::Panic(s) => write!(f, "runtime panic: {}", s),
        }
    }
}
