use crate::diagnostics::Span;

#[derive(Debug, Clone)]
pub(crate) struct ParseError {
    pub(crate) span: Span,
    pub(crate) msg: String,
}

impl ParseError {
    pub(crate) fn new(span: Span, msg: String) -> Self {
        Self { span, msg }
    }
}

impl std::fmt::Display for ParseError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.msg)
    }
}
