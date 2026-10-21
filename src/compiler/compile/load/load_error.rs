//! A failure while loading, carrying the source needed to render it.

use std::path::PathBuf;

use crate::compiler::Diagnostic;
use crate::syntax::Span;

#[derive(Debug)]
pub(crate) struct LoadError {
    pub(crate) path: PathBuf,
    pub(crate) span: Span,
    pub(crate) message: String,
    pub(crate) source: String,
}

impl LoadError {
    pub(crate) fn render(&self) -> String {
        Diagnostic::new(&self.path, self.span, &self.message).render(&self.source)
    }
}
