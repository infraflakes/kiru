//! The front half of a compile: load the program tree, resolve every name into
//! an edge, and check every body. What it leaves is the checked program, its
//! warnings, and the syntax errors recovery kept.

mod failure;

use std::path::Path;

use crate::compiler::compile::check::validate_program;
use crate::compiler::compile::load::{LoadedProgram, ProgramSource, load_files};
use crate::compiler::compile::resolve::{Analysis, resolve_tree};
use crate::compiler::diagnostic::{Diagnostic, FileDiagnostics};
use crate::compiler::model::Program;
use crate::compiler::warnings::{is_program_file, unused_bindings};
use crate::project::WarningPolicy;

use failure::{failure, failure_from};

pub use failure::BuildFailure;

/// One build that checked: the loaded program, the model the check settled,
/// and the warnings the unused syntax earned.
pub struct Build {
    pub(crate) loaded: LoadedProgram,
    pub(crate) program: Program,
    pub(crate) warnings: Vec<Diagnostic>,
    /// The syntax errors recovery kept. They withhold the output; the facts
    /// the recovered tree still earned are reported beside them.
    pub(crate) syntax: Vec<Diagnostic>,
}

impl std::fmt::Debug for Build {
    /// The build's shape: the file count and the diagnostics, not the model.
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("Build")
            .field("files", &self.loaded.files.len())
            .field("warnings", &self.warnings.len())
            .finish()
    }
}

impl Build {
    /// The program's diagnostics, file by file: every file of the program
    /// tree, with the errors and warnings it holds. The standard library's
    /// files name no real path, so they publish nothing.
    pub fn diagnostics(&self) -> Vec<FileDiagnostics> {
        let mut by_file: Vec<FileDiagnostics> = self
            .loaded
            .files
            .iter()
            .filter(|file| file.path.is_absolute())
            .map(|file| FileDiagnostics {
                path: file.path.clone(),
                source: file.source.clone(),
                diagnostics: Vec::new(),
            })
            .collect();
        for warning in &self.warnings {
            if let Some(file) = by_file.iter_mut().find(|file| file.path == warning.path) {
                file.diagnostics.push(warning.clone());
            }
        }
        by_file
    }
}

/// Run the build: load the program tree and the library through `source`,
/// resolve every name into an edge, and check every body. The one build the
/// compiler has; the source decides only what the files read as.
pub(crate) fn build(
    tree_root: &Path,
    source: &impl ProgramSource,
    policy: &WarningPolicy,
) -> Result<Build, BuildFailure> {
    let mut loaded = match load_files(tree_root, source) {
        Ok(loaded) => loaded,
        Err(error) => return Err(failure(vec![error], None)),
    };
    // A recovered tree still runs every stage: the syntax errors travel with
    // the result and the output is withheld, but the facts the recovered
    // source can still earn are facts the caller reports.
    let Analysis { program, warnings } = match resolve_tree(&loaded) {
        Ok(analysis) => analysis,
        Err(diagnostic) => {
            let mut diagnostics = loaded.syntax.clone();
            diagnostics.push(diagnostic);
            return Err(failure_from(&loaded, diagnostics));
        }
    };
    let mut program = program;
    let mut warnings = warnings;
    warnings.extend(unused_bindings(&program));
    let errors = validate_program(&mut program);
    if !errors.is_empty() {
        // The checker's errors are facts among the file's facts: the
        // recovery's syntax errors, what the recovered tree kept unused, and
        // every error the checker met all report.
        let mut diagnostics = loaded.syntax.clone();
        diagnostics.extend(errors);
        let mut program_warnings = warnings.clone();
        program_warnings.retain(|warning| {
            is_program_file(&warning.path) && warning.warning.is_none_or(|kind| policy.admits(kind))
        });
        diagnostics.extend(program_warnings);
        return Err(failure_from(&loaded, diagnostics));
    }
    let syntax = std::mem::take(&mut loaded.syntax);
    Ok(Build {
        loaded,
        program,
        warnings,
        syntax,
    })
}
