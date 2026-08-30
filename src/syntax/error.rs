use miette::{Diagnostic, SourceSpan};
use thiserror::Error;

#[derive(Debug, Error, Diagnostic)]
#[error("{msg}")]
pub struct ParseError {
    #[label("{msg}")]
    span: SourceSpan,
    msg: String,
}

impl ParseError {
    pub fn new(span: SourceSpan, msg: String) -> Self {
        Self { span, msg }
    }
}
