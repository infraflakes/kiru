//! The tree analysis: the process an editor asks for. It loads the tree the
//! file belongs to, resolves every name, links and checks every body, and
//! groups the facts by file. Checking one file must never invent a fact about
//! another: a callee's halting is settled from its body, so a body ending in a
//! call that panics is known to stop the run.
//!
//! The same model supplies the file's semantic tokens. A client chooses how
//! much of the grouped facts it publishes: one file, or the whole tree.

mod semantic;

use std::path::Path;

use crate::compiler::compile::check::validate_program;
use crate::compiler::compile::load::{LoadedProgram, ProgramSource, load_files, load_library};
use crate::compiler::compile::resolve::{Analysis, program_main, resolve_tree};
use crate::compiler::diagnostic::{Diagnostic, FileDiagnostics};
use crate::compiler::mark_reachable;
use crate::compiler::model::FileId;
use crate::compiler::warnings::{is_program_file, unused_bindings, unused_declarations};
use crate::project::{Project, WarningPolicy};

#[cfg(test)]
pub(crate) use semantic::SemanticRole;
pub(crate) use semantic::{SemanticToken, TOKEN_MODIFIERS, TOKEN_TYPES};

/// Which tree the editor's file belongs to. A project's tree is read beside
/// the embedded library; the compiler's own standard library is the one tree
/// read without it, under the `std` name.
pub(crate) enum Tree<'a> {
    Program(&'a Project),
    StandardLibrary(&'a Path),
}

impl Tree<'_> {
    /// The diagnostics policy of the tree: the project's, or the default for
    /// the compiler's own library.
    fn policy(&self) -> WarningPolicy {
        match self {
            Tree::Program(project) => project.warnings,
            Tree::StandardLibrary(_) => WarningPolicy::default(),
        }
    }
}

/// The facts of the whole tree, file by file. A file with no facts is absent;
/// the client clears what it published before.
pub(crate) fn diagnostics(tree: Tree<'_>, source: &impl ProgramSource) -> Vec<FileDiagnostics> {
    let policy = tree.policy();
    match analyzed(tree, source) {
        Ok(analyzed) => group(&analyzed.loaded, analyzed.diagnostics, policy),
        Err(diagnostics) => group_without_tree(diagnostics, policy),
    }
}

/// The roles every name of one file plays, for a client that paints the
/// source. The tree is resolved like a check because the check settles the
/// method families the tokens name.
pub(crate) fn semantic_tokens(
    path: &Path,
    source: &impl ProgramSource,
    tree: Tree<'_>,
) -> Vec<SemanticToken> {
    let Ok(analyzed) = analyzed(tree, source) else {
        return Vec::new();
    };
    let Some(index) = analyzed
        .loaded
        .files
        .iter()
        .position(|file| file.path == path)
    else {
        return Vec::new();
    };
    semantic::tokens(&analyzed.loaded, &analyzed.program, FileId(index))
}

/// One tree after resolution and checking: its files, its model, and every
/// fact the tree earned.
struct Analyzed {
    loaded: LoadedProgram,
    program: crate::compiler::model::Program,
    diagnostics: Vec<Diagnostic>,
}

/// Load, resolve, and check one tree. A load or resolution failure is the
/// facts the caller reports; the tree before the failure still names its
/// files.
fn analyzed(tree: Tree<'_>, source: &impl ProgramSource) -> Result<Analyzed, Vec<FileDiagnostics>> {
    let loaded = match tree {
        Tree::Program(project) => load_files(&project.root, source),
        Tree::StandardLibrary(root) => load_library(root, source),
    };
    let loaded = match loaded {
        Ok(loaded) => loaded,
        Err(error) => {
            let diagnostic = Diagnostic::new(&error.path, error.span, &error.message);
            return Err(vec![FileDiagnostics {
                path: error.path,
                source: error.source,
                diagnostics: vec![diagnostic],
            }]);
        }
    };
    let Analysis {
        mut program,
        warnings,
    } = match resolve_tree(&loaded) {
        Ok(analysis) => analysis,
        Err(diagnostic) => {
            let mut diagnostics = loaded.syntax.clone();
            diagnostics.push(diagnostic);
            return Err(group(&loaded, diagnostics, WarningPolicy::default()));
        }
    };
    // A recovered tree still reports: its syntax errors, every unused fact the
    // recovery kept, every error the checker met, and the entry's own fact.
    let mut diagnostics = loaded.syntax.clone();
    diagnostics.extend(validate_program(&mut program));
    diagnostics.extend(warnings);
    diagnostics.extend(unused_bindings(&program));
    let main = match program_main(&program) {
        Ok(main) => main,
        Err(diagnostic) => {
            diagnostics.push(diagnostic);
            None
        }
    };
    // A missing `main` is not an editor error: a library and a work in
    // progress have none. When one exists, the declarations it never reaches
    // are unused.
    if let Some(main) = main {
        let reachable = mark_reachable(&program, main);
        diagnostics.extend(unused_declarations(&program, &reachable));
    }
    Ok(Analyzed {
        loaded,
        program,
        diagnostics,
    })
}

/// Group facts by the file they name, keeping the source the spans convert
/// against, and drop the warning kinds the tree's policy silences.
fn group(
    loaded: &LoadedProgram,
    diagnostics: Vec<Diagnostic>,
    policy: WarningPolicy,
) -> Vec<FileDiagnostics> {
    let mut files: Vec<FileDiagnostics> = Vec::new();
    for diagnostic in diagnostics {
        // The embedded library is the compiler's, not the editor's: its files
        // have no real path, and a fact about one cannot be published.
        if !is_program_file(&diagnostic.path)
            || diagnostic.warning.is_some_and(|kind| !policy.admits(kind))
        {
            continue;
        }
        if let Some(file) = files.iter_mut().find(|file| file.path == diagnostic.path) {
            file.diagnostics.push(diagnostic);
            continue;
        }
        let source = loaded
            .files
            .iter()
            .find(|file| file.path == diagnostic.path)
            .map(|file| file.source.clone())
            .unwrap_or_default();
        files.push(FileDiagnostics {
            path: diagnostic.path.clone(),
            source,
            diagnostics: vec![diagnostic],
        });
    }
    for file in &mut files {
        file.diagnostics
            .sort_by_key(|diagnostic| diagnostic.span.start);
    }
    files.sort_by(|left, right| left.path.cmp(&right.path));
    files
}

/// Group facts that already name their files (a load or resolution failure)
/// and apply the same policy.
fn group_without_tree(files: Vec<FileDiagnostics>, policy: WarningPolicy) -> Vec<FileDiagnostics> {
    files
        .into_iter()
        .map(|mut file| {
            file.diagnostics
                .retain(|diagnostic| diagnostic.warning.is_none_or(|kind| policy.admits(kind)));
            file
        })
        .filter(|file| !file.diagnostics.is_empty())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compiler::{OverlayOnDisk, Severity, VirtualSource, WarningKind, checked_program};
    use crate::project::{DiagnosticScope, Project, WarningSetting};
    use std::path::Path;

    /// A project over a virtual tree, with the default policy.
    fn project() -> Project {
        Project {
            manifest: Path::new("/virtual/kiru.toml").to_path_buf(),
            root: Path::new("/virtual").to_path_buf(),
            output: Path::new("/virtual/program").to_path_buf(),
            scope: DiagnosticScope::File,
            warnings: WarningPolicy::default(),
        }
    }

    /// Every fact the project's whole tree earns.
    fn tree_facts(files: &[(&str, &str)]) -> Vec<Diagnostic> {
        let project = project();
        let source = VirtualSource::new(files);
        diagnostics(Tree::Program(&project), &source)
            .into_iter()
            .flat_map(|file| file.diagnostics)
            .collect()
    }

    fn messages(files: &[(&str, &str)]) -> Vec<String> {
        tree_facts(files)
            .into_iter()
            .map(|diagnostic| diagnostic.message)
            .collect()
    }

    #[test]
    fn every_files_facts_report() {
        let facts = messages(&[
            ("main.kiru", "fn main() { let unused: int = 1; };"),
            (
                "other.kiru",
                "pub fn other() -> int { let wrong: int = \"t\"; return 0; };",
            ),
        ]);
        assert!(
            facts
                .iter()
                .any(|fact| fact == "the local `unused` is never read"),
            "{facts:?}"
        );
        assert!(
            facts.iter().any(|fact| fact == "expected int, found txt"),
            "a type error in another file still reports: {facts:?}"
        );
    }

    #[test]
    fn a_call_that_panics_in_another_file_settles_the_flow() {
        // `get` ends in a call to a function whose body panics. The halting
        // fact is settled from every body, not only the file being analyzed,
        // so no false "must end with return or panic" is invented.
        let facts = messages(&[
            ("main.kiru", "fn stop() { panic; };\nfn main() { stop(); };"),
            ("task.kiru", "pub fn get() -> int { stop(); };"),
        ]);
        assert!(
            facts.iter().all(|fact| !fact.contains("must end")),
            "the call stops the run: {facts:?}"
        );
        assert!(
            facts
                .iter()
                .any(|fact| fact.contains("`root::get` is unused")),
            "the unreached declaration is still a warning: {facts:?}"
        );
    }

    #[test]
    fn a_missing_main_is_not_an_editor_error() {
        let facts = tree_facts(&[("lib.kiru", "pub fn get() -> int { panic; };")]);
        assert!(
            facts
                .iter()
                .all(|fact| !matches!(fact.severity, Severity::Error)),
            "a library needs no entry: {facts:?}"
        );
    }

    #[test]
    fn the_diagnostics_policy_silences_a_warning_kind() {
        let mut project = project();
        project.warnings.unused_local = WarningSetting::Off;
        let source = VirtualSource::new(&[("main.kiru", "fn main() { let unused: int = 1; };")]);
        let facts: Vec<Diagnostic> = diagnostics(Tree::Program(&project), &source)
            .into_iter()
            .flat_map(|file| file.diagnostics)
            .collect();
        assert!(facts.is_empty(), "the local warning is off: {facts:?}");

        project.warnings.unused_local = WarningSetting::Warning;
        let facts: Vec<Diagnostic> = diagnostics(Tree::Program(&project), &source)
            .into_iter()
            .flat_map(|file| file.diagnostics)
            .collect();
        assert_eq!(facts.len(), 1, "{facts:?}");
        assert_eq!(facts[0].warning, Some(WarningKind::UnusedLocal));
    }

    #[test]
    fn a_syntax_error_reports_beside_the_recovered_trees_facts() {
        let facts = tree_facts(&[(
            "main.kiru",
            "use std::text;\nfn main() { let unused: int = 1; let broken: int = ; };",
        )]);
        let messages: Vec<&str> = facts
            .iter()
            .map(|diagnostic| diagnostic.message.as_str())
            .collect();
        assert!(
            messages
                .iter()
                .any(|message| message.contains("expected an expression")),
            "{messages:?}"
        );
        assert!(messages.contains(&"unused use `std::text`"), "{messages:?}");
        assert!(
            messages.contains(&"the local `unused` is never read"),
            "{messages:?}"
        );
    }

    #[test]
    fn the_library_file_with_the_cross_file_call_is_clean() {
        // The real `get` that reported the false fall-through: its body ends
        // in `std::eprint(...)`, whose own body panics.
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("std");
        let root = root.canonicalize().expect("the library is readable");
        let path = root.join("dynamic/map.kiru");
        let source = std::fs::read_to_string(&path).expect("the library file is readable");
        let reports = diagnostics(
            Tree::StandardLibrary(&root),
            &OverlayOnDisk {
                path: path.clone(),
                source,
            },
        );
        let errors: Vec<_> = reports
            .iter()
            .flat_map(|file| &file.diagnostics)
            .filter(|diagnostic| matches!(diagnostic.severity, Severity::Error))
            .collect();
        assert!(errors.is_empty(), "the library file is clean: {errors:?}");
    }

    #[test]
    fn a_checked_program_still_reads_through_the_build() {
        // The build facade and the analysis share one model: a program that
        // checks through the build has no error in its analysis either.
        let program = checked_program("fn main() { let x: int = 1; std::print(\"x\"); };");
        assert!(!program.declarations.is_empty());
    }
}
