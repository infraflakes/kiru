//! The one build: the whole program a caller asks for, loaded, resolved, and
//! checked.
//!
//! A build is the compiler's first half — the stages from loading through the
//! check, in the order the model requires — and it is the only place those
//! stages are named together. The caller picks where the program's files come
//! from: the disk that `kc` reads, a virtual tree in the tests, or the editor
//! overlay the language server keeps. What the caller gets is the same either
//! way: the checked program with its warnings, or the diagnostics that name
//! why there is no program.
//!
//! The second half of the compile — pruning, lowering, packaging — belongs to
//! the pipeline, because only a compiled program needs it.

mod analyze;
mod failure;
mod semantic;
mod warnings;

use std::path::Path;

use crate::compiler::diagnostic::Diagnostic;
use crate::compiler::load_files::{LoadedProgram, ProgramSource, load_files};
use crate::compiler::model::{DeclarationId, Program};
use crate::compiler::resolve_names::{Resolved, resolve_program};
use crate::compiler::validate_program::validate_program;
use failure::{failure, failure_from};

pub(crate) use analyze::semantic_tokens;
pub use analyze::{AnalysisContext, analyze_file};
pub use failure::{BuildFailure, FileDiagnostics};
#[cfg(test)]
pub(crate) use semantic::SemanticRole;
pub(crate) use semantic::{SemanticToken, TOKEN_MODIFIERS, TOKEN_TYPES};
pub(crate) use warnings::{is_program_file, unused_bindings, unused_declarations};

/// One build that checked: the loaded program, the model the check settled,
/// the entry it starts from, and the warnings the unused syntax earned.
pub struct Build {
    pub(crate) loaded: LoadedProgram,
    pub(crate) program: Program,
    pub(crate) entry: DeclarationId,
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
/// resolve every name into an edge, select the entry, and check every body.
/// The one pipeline the compiler has; the source decides only what the files
/// read as.
pub fn build(entry: &Path, source: &impl ProgramSource) -> Result<Build, BuildFailure> {
    let mut loaded = match load_files(entry, source) {
        Ok(loaded) => loaded,
        Err(error) => return Err(failure(vec![error], None)),
    };
    // A recovered tree still runs every stage: the syntax errors travel with
    // the result and the output is withheld, but the facts the recovered
    // source can still earn are facts the caller reports.
    let Resolved {
        program,
        entry,
        warnings,
    } = match resolve_program(&loaded) {
        Ok(resolved) => resolved,
        Err(diagnostic) => {
            let mut diagnostics = loaded.syntax.clone();
            diagnostics.push(diagnostic);
            return Err(failure_from(&loaded, diagnostics));
        }
    };
    let mut program = program;
    let mut warnings = warnings;
    warnings.extend(unused_bindings(&program, None));
    if let Err(diagnostic) = validate_program(&mut program) {
        // The checker's error is still one fact among the file's facts: the
        // recovery's syntax errors, what the recovered tree kept unused, and
        // the error itself all report.
        let mut diagnostics = loaded.syntax.clone();
        diagnostics.push(diagnostic);
        let mut program_warnings = warnings.clone();
        program_warnings.retain(|warning| is_program_file(&warning.path));
        diagnostics.extend(program_warnings);
        return Err(failure_from(&loaded, diagnostics));
    }
    let syntax = std::mem::take(&mut loaded.syntax);
    Ok(Build {
        loaded,
        program,
        entry,
        warnings,
        syntax,
    })
}
