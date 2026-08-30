use crate::diagnostics::Span;

#[derive(Debug, Clone)]
pub struct ParseError {
    pub span: Span,
    pub msg: String,
}

impl ParseError {
    pub fn new(span: Span, msg: String) -> Self {
        Self { span, msg }
    }
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.msg)
    }
}

impl std::error::Error for ParseError {}
