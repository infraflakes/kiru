//! The build of one file, and the semantic tokens of it: the analysis a client
//! asks for while an editor holds the file. It resolves the file in the context
//! the client names and checks only that file's bodies.

use std::path::Path;

use crate::compiler::diagnostic::Diagnostic;
use crate::compiler::load_files::{LoadedProgram, ProgramSource, load_files, load_single};
use crate::compiler::model::{FileId, Program};
use crate::compiler::resolve_names::{Analysis, resolve_file};
use crate::compiler::validate_program::validate_file;

use super::semantic::{self, SemanticToken};
use super::warnings::unused_bindings;

/// program tree the client names, or a file of a library tree.
#[derive(Clone, Copy)]
pub enum AnalysisContext<'a> {
    /// The file alone: the library is mounted and nothing else on disk joins
    /// it. A program name that is not the file's own is out of scope.
    Single,
    /// The file inside the program tree rooted at this directory. The tree is
    /// loaded so every namespace the file writes resolves; only the analyzed
    /// file's bodies are checked.
    Program(&'a Path),
    /// The file inside the library rooted at this directory. The library is
    /// read from disk with the analyzed file's buffer in place of its own.
    Library(&'a Path),
}

pub fn analyze_file(
    path: &Path,
    source: &impl ProgramSource,
    context: AnalysisContext<'_>,
) -> Vec<Diagnostic> {
    let Analyzed {
        loaded,
        mut program,
        warnings,
        file,
    } = match analyzed(path, source, context) {
        Ok(analyzed) => analyzed,
        Err(diagnostics) => return diagnostics,
    };
    // A recovered tree still reports: its syntax errors, the unused facts
    // recovery kept, and the first error the checker meets in what remains.
    let mut syntax: Vec<Diagnostic> = loaded
        .syntax
        .iter()
        .filter(|diagnostic| diagnostic.path == path)
        .cloned()
        .collect();
    let mut warnings = warnings;
    warnings.extend(unused_bindings(&program, Some(file)));
    warnings.retain(|diagnostic| diagnostic.path == path);
    if !syntax.is_empty() {
        if let Err(diagnostic) = validate_file(&mut program, file) {
            syntax.extend(warnings);
            syntax.push(diagnostic);
        } else {
            syntax.extend(warnings);
        }
        return syntax;
    }
    if let Err(diagnostic) = validate_file(&mut program, file) {
        warnings.push(diagnostic);
    }
    warnings
}

/// The roles every name of one file plays, for a client that paints the
/// source. The file is resolved like a check and validated, because the check
/// settles the method families the tokens name.
pub(crate) fn semantic_tokens(
    path: &Path,
    source: &impl ProgramSource,
    context: AnalysisContext<'_>,
) -> Vec<SemanticToken> {
    let Analyzed {
        loaded,
        mut program,
        file,
        ..
    } = match analyzed(path, source, context) {
        Ok(analyzed) => analyzed,
        Err(_) => return Vec::new(),
    };
    let _ = validate_file(&mut program, file);
    semantic::tokens(&loaded, &program, file)
}

/// One file after loading and resolution: its source, its model, the
/// resolution's warnings, and where the file stands in the load.
struct Analyzed {
    loaded: LoadedProgram,
    program: Program,
    warnings: Vec<Diagnostic>,
    file: FileId,
}

/// Load one file and resolve it, with no entry: the facts both the diagnostics
/// and the semantic tokens read. A load or resolution error is the file's
/// diagnostic list.
fn analyzed(
    path: &Path,
    source: &impl ProgramSource,
    context: AnalysisContext<'_>,
) -> Result<Analyzed, Vec<Diagnostic>> {
    let loaded = match context {
        AnalysisContext::Single => load_single(path, source, None),
        AnalysisContext::Library(root) => load_single(path, source, Some(root)),
        AnalysisContext::Program(root) => load_files(root, source),
    };
    let loaded = match loaded {
        Ok(loaded) => loaded,
        // A tree that cannot be read at all names the file it stopped on. It
        // is the analyzed file's fact only when it is that file.
        Err(error) if error.path == path => {
            return Err(vec![Diagnostic::new(error.path, error.span, error.message)]);
        }
        Err(_) => return Err(Vec::new()),
    };
    let Some(index) = loaded.files.iter().position(|file| file.path == path) else {
        return Err(Vec::new());
    };
    let file = FileId(index);
    let Analysis { program, warnings } = match resolve_file(&loaded, file) {
        Ok(analysis) => analysis,
        // A tree whose resolution stops on another file does not make that
        // file's failure the analyzed file's diagnostic.
        Err(diagnostic) => {
            let mut diagnostics: Vec<Diagnostic> = loaded
                .syntax
                .iter()
                .filter(|diagnostic| diagnostic.path == path)
                .cloned()
                .collect();
            if diagnostic.path == path {
                diagnostics.push(diagnostic);
            }
            return Err(diagnostics);
        }
    };
    Ok(Analyzed {
        loaded,
        program,
        warnings,
        file,
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compiler::Severity;
    use crate::compiler::build;
    use crate::compiler::{OverlayOnDisk, VirtualSource};
    use crate::syntax::Span;
    use std::path::PathBuf;

    #[test]
    fn a_file_is_checked_in_its_own_context() {
        let cases = [
            (
                "an unused local",
                "fn main() { let x: int = 1; };",
                "Warning",
                "the local `x` is never read",
            ),
            (
                "an unused use",
                "use std::text;\nfn main() {};",
                "Warning",
                "unused use `std::text`",
            ),
            (
                "a wrong type",
                "fn main() { let x: int = \"t\"; };",
                "Error",
                "expected int, found txt",
            ),
            (
                "a wrong arity",
                "fn f(a: int) {};\nfn main() { f(1, 2); };",
                "Error",
                "`root::f` takes 1 argument, found 2",
            ),
            (
                "a syntax error",
                "fn main() {",
                "Error",
                "expected `}` to close the block",
            ),
        ];
        for (case, source, severity, message) in cases {
            let facts: Vec<(String, String)> = analyze_file(
                Path::new("/virtual/main.kiru"),
                &VirtualSource::new(&[("main.kiru", source)]),
                AnalysisContext::Single,
            )
            .into_iter()
            .map(|diagnostic| (format!("{:?}", diagnostic.severity), diagnostic.message))
            .collect();
            assert!(
                facts
                    .iter()
                    .any(|(level, text)| level == severity && text.contains(message)),
                "{case}: expected a {severity} naming `{message}`, found {facts:?}"
            );
        }
    }

    #[test]
    fn a_syntax_error_reports_the_files_unused_facts_without_checking() {
        let source = "use std::text;\nfn main() { let unused: int = 1; let broken: int = ; };";
        let diagnostics = analyze_file(
            Path::new("/virtual/main.kiru"),
            &VirtualSource::new(&[("main.kiru", source)]),
            AnalysisContext::Single,
        );
        let facts: Vec<(String, String)> = diagnostics
            .iter()
            .map(|diagnostic| {
                (
                    format!("{:?}", diagnostic.severity),
                    diagnostic.message.clone(),
                )
            })
            .collect();
        assert_eq!(
            facts.len(),
            3,
            "the syntax error and both unused facts: {facts:?}"
        );
        assert!(
            facts
                .iter()
                .any(|(level, text)| level == "Error" && text.contains("expected an expression")),
            "{facts:?}"
        );
        assert!(
            facts
                .iter()
                .any(|(level, text)| level == "Warning" && text == "unused use `std::text`"),
            "{facts:?}"
        );
        assert!(
            facts
                .iter()
                .any(|(level, text)| level == "Warning"
                    && text == "the local `unused` is never read"),
            "{facts:?}"
        );
    }

    #[test]
    fn a_recovered_tree_still_reports_its_type_error() {
        let source = "use std::text;\nfn main() { let wrong: int = \"t\"; let broken: int = ; };";
        let diagnostics = analyze_file(
            Path::new("/virtual/main.kiru"),
            &VirtualSource::new(&[("main.kiru", source)]),
            AnalysisContext::Single,
        );
        let messages: Vec<&str> = diagnostics
            .iter()
            .map(|diagnostic| diagnostic.message.as_str())
            .collect();
        assert!(
            messages
                .iter()
                .any(|message| message.contains("expected an expression")),
            "the syntax error reports: {messages:?}"
        );
        assert!(
            messages.contains(&"expected int, found txt"),
            "the checker still reports what the recovered tree earned: {messages:?}"
        );
    }

    #[test]
    fn a_program_tree_resolves_the_namespaces_beside_the_file() {
        // The file's `use root::kiru` names the directory beside it, and the
        // module value it declares under the same name is no use target: only
        // a namespace, a type, or a function is.
        let files = [
            (
                "main.kiru",
                "use root::kiru;\nlet kiru: kiru::Project = { name = \"k\" };\nfn main() { kiru::run(); };\n",
            ),
            (
                "kiru/lib.kiru",
                "pub rec Project { pub name: txt };\npub fn run() {};\n",
            ),
        ];
        let facts = analyze_file(
            Path::new("/virtual/main.kiru"),
            &VirtualSource::new(&files),
            AnalysisContext::Program(Path::new("/virtual")),
        );
        assert!(
            facts.is_empty(),
            "the sibling namespace resolves and nothing is wrong: {facts:?}"
        );
    }

    #[test]
    fn a_clean_file_still_reports_its_type_error() {
        let diagnostics = analyze_file(
            Path::new("/virtual/main.kiru"),
            &VirtualSource::new(&[("main.kiru", "fn main() { let x: int = \"t\"; };")]),
            AnalysisContext::Single,
        );
        assert!(
            diagnostics
                .iter()
                .any(|diagnostic| diagnostic.message == "expected int, found txt"),
            "{diagnostics:?}"
        );
    }

    #[test]
    fn a_library_file_is_checked_against_its_library() {
        // The library's own file: it joins the namespace its directories
        // name, so `impl Vec` resolves without an entry or a program. The
        // source holds its text in memory; the file on disk is not edited.
        let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("std");
        let path = root.join("dynamic/vec.kiru");
        let source = std::fs::read_to_string(&path).expect("the library file is readable");
        let facts = analyze_file(
            &path,
            &OverlayOnDisk {
                path: path.clone(),
                source,
            },
            AnalysisContext::Library(&root),
        );
        assert!(
            facts
                .iter()
                .all(|diagnostic| !matches!(diagnostic.severity, Severity::Error)),
            "the library's own file checks without errors: {facts:?}"
        );
    }

    /// The build of a virtual tree, with every file's diagnostics flattened
    /// to (path, severity, message).
    fn built(files: &[(&str, &str)]) -> Vec<(String, Severity, String)> {
        build(Path::new("main.kiru"), &VirtualSource::new(files))
            .expect("the program checks")
            .diagnostics()
            .into_iter()
            .flat_map(|file| {
                file.diagnostics.into_iter().map(move |diagnostic| {
                    (
                        file.path.to_string_lossy().to_string(),
                        diagnostic.severity,
                        diagnostic.message,
                    )
                })
            })
            .collect()
    }

    #[test]
    fn a_program_without_uses_or_locals_publishes_nothing() {
        let facts = built(&[("main.kiru", "fn main() {};")]);
        assert!(facts.is_empty(), "expected nothing, found {facts:?}");
    }

    #[test]
    fn a_use_read_through_its_qualifier_is_not_unused() {
        let files = "use std::dynamic;\nfn main() { let xs: dynamic::Vec<int> = dynamic::Vec.new(); xs.push(1); };";
        let facts = built(&[("main.kiru", files)]);
        assert!(facts.is_empty(), "expected nothing, found {facts:?}");
    }

    #[test]
    fn the_uses_no_body_reads_are_warnings_at_their_items() {
        // An absolute path resolves on its own, with no use, so both `use`s
        // here are never read: `use std;` delivers the tree name `std` a
        // qualified path names without it, and `use std::text;` delivers the
        // namespace `std::text` no body reads.
        let files = "use std;\nuse std::text;\nfn main() { std::print(\"hi\"); };";
        let built = build(
            Path::new("main.kiru"),
            &VirtualSource::new(&[("main.kiru", files)]),
        )
        .expect("the program checks");
        let warnings = &built.diagnostics()[0].diagnostics;
        assert_eq!(warnings.len(), 2, "both `use`s are unused");
        let first = &warnings[0];
        assert_eq!(first.path, PathBuf::from("/virtual/main.kiru"));
        assert_eq!(first.span, Span::new(0, 8));
        assert_eq!(first.severity, Severity::Warning);
        assert_eq!(first.message, "unused use `std`");
        let second = &warnings[1];
        assert_eq!(second.span, Span::new(9, 23));
        assert_eq!(second.message, "unused use `std::text`");
    }

    #[test]
    fn a_use_read_in_another_file_of_its_namespace_is_not_unused() {
        // The `use` stands in help.kiru; the body that reads its delivery is
        // main.kiru's. A use belongs to its namespace, not to its file.
        let facts = built(&[
            (
                "main.kiru",
                "fn main() { let xs: dynamic::Vec<int> = dynamic::Vec.new(); xs.push(1); };",
            ),
            ("help.kiru", "use std::dynamic;"),
        ]);
        assert!(facts.is_empty(), "expected nothing, found {facts:?}");
    }

    #[test]
    fn a_use_resolved_only_by_its_absolute_path_is_unused() {
        let files = "use root::help;\nfn help() -> txt { return \"helped\"; };\nfn main() { std::print(root::help()); };";
        let facts = built(&[("main.kiru", files)]);
        assert_eq!(
            facts,
            vec![(
                "/virtual/main.kiru".to_owned(),
                Severity::Warning,
                "unused use `root::help`".to_owned()
            )]
        );
    }

    #[test]
    fn an_unused_local_binding_is_a_warning_at_its_name() {
        let files = "fn main() { let x: txt = \"a\"; };";
        let built = build(
            Path::new("main.kiru"),
            &VirtualSource::new(&[("main.kiru", files)]),
        )
        .expect("the program checks");
        // The published diagnostics name the program's files only; the
        // standard library's files are never published.
        let published = built.diagnostics();
        assert_eq!(published.len(), 1);
        let warnings = &published[0].diagnostics;
        assert_eq!(warnings.len(), 1, "one local is unused");
        let warning = &warnings[0];
        // The span is the name `x`, at the binding statement's name.
        assert_eq!(warning.span, Span::new(16, 17));
        assert_eq!(warning.severity, Severity::Warning);
        assert_eq!(warning.message, "the local `x` is never read");
    }

    #[test]
    fn a_binding_only_assigned_is_never_read() {
        let facts = built(&[("main.kiru", "fn main() { let x: int = 1; x = 2; };")]);
        assert_eq!(facts.len(), 1, "the assignment is not a read");
        assert_eq!(facts[0].1, Severity::Warning);
        assert_eq!(facts[0].2, "the local `x` is never read");
    }

    #[test]
    fn a_parameter_never_read_is_a_warning() {
        let files = "fn greet(name: txt) -> txt { return \"hi\"; };\nfn main() { greet(\"a\"); };";
        let facts = built(&[("main.kiru", files)]);
        assert_eq!(facts.len(), 1, "the parameter is unused");
        assert_eq!(facts[0].1, Severity::Warning);
        assert_eq!(facts[0].2, "the local `name` is never read");
    }

    #[test]
    fn a_local_read_only_inside_a_closure_is_read() {
        let facts = built(&[
            (
                "main.kiru",
                "use root::inner::help;\nfn main() { let f: fn() -> txt = fn() -> txt { return help(); }; let said: txt = f(); std::print(said); };",
            ),
            (
                "inner/help.kiru",
                "pub fn help() -> txt { return \"helped\"; };",
            ),
        ]);
        assert!(facts.is_empty(), "expected nothing, found {facts:?}");
    }

    #[test]
    fn a_duplicate_declaration_points_at_the_first_declaration() {
        let files = "fn help() {};\nfn help() {};\nfn main() {};";
        let failure = build(
            Path::new("main.kiru"),
            &VirtualSource::new(&[("main.kiru", files)]),
        )
        .expect_err("the duplicate is rejected");
        let files_reported = failure.diagnostics();
        assert_eq!(files_reported.len(), 1, "the duplicate names one file");
        let file = &files_reported[0];
        assert_eq!(file.path, PathBuf::from("/virtual/main.kiru"));
        // The source the build read the file as, carried for the boundary.
        assert_eq!(file.source, "fn help() {};\nfn help() {};\nfn main() {};");
        assert_eq!(file.diagnostics.len(), 1, "the failure names one fact");
        let diagnostic = &file.diagnostics[0];
        assert_eq!(diagnostic.severity, Severity::Error);
        // The second `help`'s name.
        assert_eq!(diagnostic.span, Span::new(17, 21));
        assert_eq!(
            diagnostic.related.len(),
            1,
            "the first declaration is attached"
        );
        let related = &diagnostic.related[0];
        assert_eq!(related.path, PathBuf::from("/virtual/main.kiru"));
        assert_eq!(related.span, Span::new(3, 7));
        assert_eq!(related.message, "first declaration of `help`");
    }
}
