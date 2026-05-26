use std::fmt;

#[derive(Debug)]
pub(crate) enum RuntimeError {
    Lookup(String),
    Io(std::io::Error),
    Exec {
        cmd: String,
        exit_code: Option<i32>,
        detail: String,
    },
    Panic(String),
    Other(String),
}

impl RuntimeError {
    pub(crate) fn exec_io_error(cmd: impl ToString, err: impl ToString) -> Self {
        RuntimeError::Exec {
            cmd: cmd.to_string(),
            exit_code: None,
            detail: err.to_string(),
        }
    }

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
            RuntimeError::Other(s) => write!(f, "{}", s),
        }
    }
}

impl std::error::Error for RuntimeError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            RuntimeError::Io(e) => Some(e),
            _ => None,
        }
    }
}
