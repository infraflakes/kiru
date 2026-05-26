use std::fmt;

#[derive(Debug)]
pub struct RuntimeError(pub(crate) String);

impl RuntimeError {
    pub fn new(msg: impl Into<String>) -> Self {
        RuntimeError(msg.into())
    }
}

impl fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl std::error::Error for RuntimeError {}
