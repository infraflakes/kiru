//! The tree analysis: the process an editor asks for. It loads the tree the
//! file belongs to, resolves every name, links and checks every body, and
//! groups the facts by file. Checking one file must never invent a fact about
//! another: a callee's halting is settled from its body, so a body ending in a
//! call that panics is known to stop the run.
//!
//! A diagnostics request pays for the unused facts; a token request does not.
//! Both share the load, the resolution, and the check, so neither can disagree
//! with the other about what the tree means.

mod semantic;

use std::path::Path;

use crate::compiler::compile::check::validate_program;
use crate::compiler::compile::load::{LoadedProgram, ProgramSource, load_files, load_library};
use crate::compiler::compile::resolve::{Analysis, program_main, resolve_tree};
use crate::compiler::diagnostic::{Diagnostic, FileDiagnostics, group};
use crate::compiler::mark_reachable;
use crate::compiler::model::{FileId, Program};
use crate::compiler::warnings::{is_program_source, unused_bindings, unused_declarations};
use crate::project::WarningPolicy;

#[cfg(test)]
pub(crate) use semantic::SemanticRole;
pub(crate) use semantic::{SemanticToken, TOKEN_MODIFIERS, TOKEN_TYPES};

/// Which tree the editor's file belongs to. A project's tree is read beside
/// the embedded library; the compiler's own standard library is the one tree
/// read without it, under the `std` name.
pub(crate) enum Tree<'a> {
    Program {
        root: &'a Path,
        policy: WarningPolicy,
    },
    StandardLibrary(&'a Path),
}

impl Tree<'_> {
    /// The diagnostics policy of the tree: the project's, or the default for
    /// the compiler's own library.
    fn policy(&self) -> WarningPolicy {
        match self {
            Tree::Program { policy, .. } => *policy,
            Tree::StandardLibrary(_) => WarningPolicy::default(),
        }
    }
}

/// The facts of the whole tree, file by file. A file with no facts is absent;
/// the client clears what it published before.
pub(crate) fn diagnostics(tree: Tree<'_>, source: &impl ProgramSource) -> Vec<FileDiagnostics> {
    let policy = tree.policy();
    let linked = match link(tree, source) {
        Ok(linked) => linked,
        // A tree that cannot be resolved names only errors, and an error is
        // never silenced by the project's policy.
        Err(files) => return files,
    };
    // A recovered tree still reports: its syntax errors, every error the
    // checker met, and every unused fact the recovery kept.
    let mut diagnostics = linked.loaded.syntax.clone();
    diagnostics.extend(linked.errors);
    diagnostics.extend(linked.warnings);
    diagnostics.extend(unused_bindings(&linked.program));
    let main = match program_main(&linked.program) {
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
        let reachable = mark_reachable(&linked.program, main);
        diagnostics.extend(unused_declarations(&linked.program, &reachable));
    }
    // The embedded library is the compiler's, not the editor's: its files
    // have no real path, and a fact about one cannot be published.
    diagnostics
        .retain(|diagnostic| is_program_source(&diagnostic.path) && policy.admits_fact(diagnostic));
    diagnostics.sort_by(|left, right| {
        left.path
            .cmp(&right.path)
            .then(left.span.start.cmp(&right.span.start))
    });
    group(&linked.loaded, diagnostics)
}

/// The roles every name of one file plays, for a client that paints the
/// source. The tree is checked because the check settles the method families
/// the tokens name; the unused facts are not computed, because tokens do not
/// read them.
pub(crate) fn semantic_tokens(
    path: &Path,
    source: &impl ProgramSource,
    tree: Tree<'_>,
) -> Vec<SemanticToken> {
    let Ok(linked) = link(tree, source) else {
        return Vec::new();
    };
    let Some(index) = linked
        .loaded
        .files
        .iter()
        .position(|file| file.path == path)
    else {
        return Vec::new();
    };
    semantic::tokens(&linked.loaded, &linked.program, FileId(index))
}

/// One tree after resolution and checking: its files, its model, the
/// resolution's warnings, and every error the checker met.
struct Linked {
    loaded: LoadedProgram,
    program: Program,
    warnings: Vec<Diagnostic>,
    errors: Vec<Diagnostic>,
}

/// Load, resolve, and check one tree. A load or resolution failure is the
/// facts the caller reports; the tree before the failure still names its
/// files.
fn link(tree: Tree<'_>, source: &impl ProgramSource) -> Result<Linked, Vec<FileDiagnostics>> {
    let loaded = match tree {
        Tree::Program { root, .. } => load_files(root, source),
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
            diagnostics.retain(|diagnostic| is_program_source(&diagnostic.path));
            return Err(group(&loaded, diagnostics));
        }
    };
    let errors = validate_program(&mut program);
    Ok(Linked {
        loaded,
        program,
        warnings,
        errors,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cli::DiskSource;
    use crate::compiler::{Severity, VirtualSource, WarningKind};
    use crate::project::WarningSetting;
    use std::path::Path;

    /// Every fact a virtual project's whole tree earns.
    fn tree_facts(files: &[(&str, &str)]) -> Vec<Diagnostic> {
        let source = VirtualSource::new(files);
        let tree = Tree::Program {
            root: source.root(),
            policy: WarningPolicy::default(),
        };
        diagnostics(tree, &source)
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
                .all(|fact| !matches!(fact.severity(), Severity::Error)),
            "a library needs no entry: {facts:?}"
        );
    }

    #[test]
    fn the_diagnostics_policy_silences_a_warning_kind() {
        let source = VirtualSource::new(&[("main.kiru", "fn main() { let unused: int = 1; };")]);
        let policy = WarningPolicy {
            unused_local: WarningSetting::Off,
            ..Default::default()
        };
        let facts: Vec<Diagnostic> = diagnostics(
            Tree::Program {
                root: source.root(),
                policy,
            },
            &source,
        )
        .into_iter()
        .flat_map(|file| file.diagnostics)
        .collect();
        assert!(facts.is_empty(), "the local warning is off: {facts:?}");

        let facts: Vec<Diagnostic> = diagnostics(
            Tree::Program {
                root: source.root(),
                policy: WarningPolicy::default(),
            },
            &source,
        )
        .into_iter()
        .flat_map(|file| file.diagnostics)
        .collect();
        assert_eq!(facts.len(), 1, "{facts:?}");
        assert_eq!(facts[0].warning_kind(), Some(WarningKind::UnusedLocal));
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
    fn a_duplicate_use_is_redundant_not_unused() {
        // Uses belong to the namespace: two files may write the same one. The
        // read makes the delivery used; the second statement is the redundant
        // one, and it is named as redundant rather than unused.
        let facts = messages(&[
            (
                "main.kiru",
                "use std::process;\nfn main() { std::print(process::quote(\"x\")); };",
            ),
            ("other.kiru", "use std::process;\n"),
        ]);
        assert!(
            facts.contains(&"redundant use `std::process`".to_owned()),
            "{facts:?}"
        );
        assert!(
            facts.iter().all(|fact| !fact.contains("unused use")),
            "the delivery is read: {facts:?}"
        );
    }

    #[test]
    fn an_unread_duplicate_reports_both_facts() {
        let facts = messages(&[
            ("main.kiru", "use std::process;\nfn main() {};"),
            ("other.kiru", "use std::process;\n"),
        ]);
        assert!(
            facts.contains(&"unused use `std::process`".to_owned()),
            "the delivery is never read: {facts:?}"
        );
        assert!(
            facts.contains(&"redundant use `std::process`".to_owned()),
            "the second item duplicates the first: {facts:?}"
        );
    }

    #[test]
    fn the_policy_silences_a_redundant_use() {
        let source = VirtualSource::new(&[
            (
                "main.kiru",
                "use std::process;\nfn main() { std::print(process::quote(\"x\")); };",
            ),
            ("other.kiru", "use std::process;\n"),
        ]);
        let policy = WarningPolicy {
            redundant_use: WarningSetting::Off,
            ..Default::default()
        };
        let facts: Vec<Diagnostic> = diagnostics(
            Tree::Program {
                root: source.root(),
                policy,
            },
            &source,
        )
        .into_iter()
        .flat_map(|file| file.diagnostics)
        .collect();
        assert!(facts.is_empty(), "the redundant fact is off: {facts:?}");
    }

    #[test]
    fn a_method_receiver_is_not_an_unused_local() {
        let facts = messages(&[(
            "main.kiru",
            "rec Box { value: int };\n\
             impl Box {\n\
               fn nothing(self: Box) -> int { return 1; };\n\
               fn ignored(self: Box, unused: int) -> int { return 2; };\n\
             };\n\
             fn main() { let b: Box = { value = 1 }; b.nothing(); b.ignored(0); };",
        )]);
        assert!(
            facts.iter().all(|fact| !fact.contains("`self`")),
            "the receiver is passed by the call, not an unused local: {facts:?}"
        );
        assert!(
            facts
                .iter()
                .any(|fact| fact == "the local `unused` is never read"),
            "every other parameter still reports: {facts:?}"
        );
    }

    #[test]
    fn the_library_file_with_the_cross_file_call_is_clean() {
        // The real `get` that reported the false fall-through: its body ends
        // in `std::eprint(...)`, whose own body panics.
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("std");
        let root = root.canonicalize().expect("the library is readable");
        let reports = diagnostics(Tree::StandardLibrary(&root), &DiskSource);
        assert!(
            reports.is_empty(),
            "the library reports nothing: {reports:?}"
        );
    }
}
