//! The failure a build stopped on: its diagnostics, file by file, and the
//! rendering a terminal prints for them.

use std::path::PathBuf;

use crate::compiler::compile::load::{LoadError, LoadedProgram};
use crate::compiler::diagnostic::{Diagnostic, FileDiagnostics, render};

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

    /// The source of every file the build loaded, for a boundary that
    /// converts spans against the text the build read.
    pub fn file_sources(&self) -> Vec<(PathBuf, String)> {
        self.diagnostics
            .iter()
            .map(|file| (file.path.clone(), file.source.clone()))
            .collect()
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
    let mut diagnostics: Vec<FileDiagnostics> = Vec::new();
    for diagnostic in &gathered {
        if !diagnostic.path.is_absolute() {
            continue;
        }
        if let Some(file) = diagnostics
            .iter_mut()
            .find(|file| file.path == diagnostic.path)
        {
            file.diagnostics.push(diagnostic.clone());
            continue;
        }
        let source = loaded
            .files
            .iter()
            .find(|file| file.path == diagnostic.path)
            .map(|file| file.source.clone())
            .unwrap_or_default();
        diagnostics.push(FileDiagnostics {
            path: diagnostic.path.clone(),
            source,
            diagnostics: vec![diagnostic.clone()],
        });
    }
    BuildFailure {
        diagnostics,
        rendered,
    }
}

/// Assemble the failure a build stopped on before it had a program: the
/// diagnostics placed in their files, and the rendering a terminal prints. A
/// load failure renders from the source the load carried; a check failure
/// renders from the loaded program.
pub(super) fn failure(
    errors: Vec<LoadError>,
    checked: Option<(LoadedProgram, Diagnostic)>,
) -> BuildFailure {
    let rendered = match &checked {
        Some((loaded, diagnostic)) => render(loaded, diagnostic),
        None => errors.iter().map(LoadError::render).collect(),
    };
    let diagnostics: Vec<FileDiagnostics> = match checked {
        // A check failure names a file of the program tree; the standard
        // library's files publish nothing.
        Some((loaded, diagnostic)) => {
            let source = loaded
                .files
                .iter()
                .find(|file| file.path == diagnostic.path)
                .map(|file| file.source.clone())
                .unwrap_or_default();
            let path = diagnostic.path.clone();
            if path.is_absolute() {
                vec![FileDiagnostics {
                    path,
                    source,
                    diagnostics: vec![diagnostic],
                }]
            } else {
                Vec::new()
            }
        }
        None => errors
            .iter()
            .map(|error| FileDiagnostics {
                path: error.path.clone(),
                source: error.source.clone(),
                diagnostics: vec![Diagnostic::new(&error.path, error.span, &error.message)],
            })
            .collect(),
    };
    BuildFailure {
        diagnostics,
        rendered,
    }
}
