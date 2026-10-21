//! The failure a build stopped on: its diagnostics, file by file, and the
//! rendering a terminal prints for them.

use crate::compiler::compile::load::{LoadError, LoadedProgram};
use crate::compiler::diagnostic::{Diagnostic, FileDiagnostics, group, render};

/// Why there is no checked program. The diagnostics are the facts the build
/// stopped on, placed in their files; `rendered` is what a terminal prints
/// for them.
#[derive(Debug)]
pub struct BuildFailure {
    diagnostics: Vec<FileDiagnostics>,
    rendered: String,
}

impl BuildFailure {
    /// The failure's diagnostics, file by file, the way a build reports them.
    pub fn diagnostics(&self) -> Vec<FileDiagnostics> {
        self.diagnostics.clone()
    }

    /// What a terminal prints for the failure.
    pub fn rendered(&self) -> &str {
        &self.rendered
    }
}

/// Assemble the failure a build stopped on from every diagnostic it
/// gathered, rendered in the order it gathered them. The program's files
/// carry the sources the spans name; the library's files publish nothing.
pub(super) fn failure_from(loaded: &LoadedProgram, gathered: Vec<Diagnostic>) -> BuildFailure {
    let rendered = gathered
        .iter()
        .map(|diagnostic| render(loaded, diagnostic))
        .collect();
    // The standard library's files name no real path, so they publish
    // nothing; they are still rendered above.
    let published = gathered
        .iter()
        .filter(|diagnostic| diagnostic.path.is_absolute())
        .cloned();
    BuildFailure {
        diagnostics: group(loaded, published),
        rendered,
    }
}

/// Assemble the failure a build stopped on before it had a program: a load
/// failure carries the source it stopped on.
pub(super) fn failure(errors: Vec<LoadError>) -> BuildFailure {
    let rendered = errors.iter().map(LoadError::render).collect();
    let diagnostics = errors
        .iter()
        .map(|error| FileDiagnostics {
            path: error.path.clone(),
            source: error.source.clone(),
            diagnostics: vec![Diagnostic::new(&error.path, error.span, &error.message)],
        })
        .collect();
    BuildFailure {
        diagnostics,
        rendered,
    }
}
