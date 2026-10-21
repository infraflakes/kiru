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

use std::path::{Path, PathBuf};

use crate::compiler::diagnostic::{Diagnostic, render};
use crate::compiler::load_files::{LoadError, LoadedProgram, load_files, load_single};
use crate::compiler::model::{DeclarationId, FileId, Program};
use crate::compiler::resolve_names::{Analysis, Resolved, resolve_file, resolve_program};
use crate::compiler::sources::ProgramSource;
use crate::compiler::warnings::{is_program_file, unused_bindings};
use crate::compiler::{validate_file, validate_program};

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

/// The diagnostics of one file, as a build reports them: the file, the
/// source the build read it as, and the facts it holds.
#[derive(Clone, Debug)]
pub struct FileDiagnostics {
    pub path: PathBuf,
    pub source: String,
    pub diagnostics: Vec<Diagnostic>,
}

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

/// Analyze one file by itself: the file's unused-`use` and unused-binding
/// warnings, and the first error the checker meets. The file is checked in its
/// own context: the library it belongs to is read through `source` when a
/// library root is named, the embedded library is mounted when none is, and no
/// entry is selected, so a client asks this for the buffer the editor holds
/// whatever the file is called and wherever it stands.
pub fn analyze_file(
    path: &Path,
    source: &impl ProgramSource,
    library: Option<&Path>,
) -> Vec<Diagnostic> {
    let loaded = match load_single(path, source, library) {
        Ok(loaded) => loaded,
        Err(error) => return vec![Diagnostic::new(error.path, error.span, error.message)],
    };
    let Some(file) = loaded.files.iter().position(|file| file.path == path) else {
        return Vec::new();
    };
    let file = FileId(file);
    // A recovered tree still reports: its syntax errors, the unused facts
    // recovery kept, and the first error the checker meets in what remains.
    let mut syntax: Vec<Diagnostic> = loaded
        .syntax
        .iter()
        .filter(|diagnostic| diagnostic.path == path)
        .cloned()
        .collect();
    if !syntax.is_empty() {
        match resolve_file(&loaded, file) {
            Ok(Analysis {
                mut program,
                warnings,
            }) => {
                syntax.extend(warnings);
                syntax.extend(unused_bindings(&program));
                if let Err(diagnostic) = validate_file(&mut program, file) {
                    syntax.push(diagnostic);
                }
            }
            Err(diagnostic) => syntax.push(diagnostic),
        }
        syntax.retain(|diagnostic| diagnostic.path == path);
        return syntax;
    }
    let Analysis {
        mut program,
        mut warnings,
    } = match resolve_file(&loaded, file) {
        Ok(resolved) => resolved,
        Err(diagnostic) => return vec![diagnostic],
    };
    warnings.extend(unused_bindings(&program));
    if let Err(diagnostic) = validate_file(&mut program, file) {
        warnings.push(diagnostic);
    }
    // The analyzed file's facts alone: the library is checked with it, and its
    // own warnings name files the client did not ask about.
    warnings.retain(|diagnostic| diagnostic.path == path);
    warnings
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
    warnings.extend(unused_bindings(&program));
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

/// Assemble the failure a build stopped on from every diagnostic it
/// gathered, rendered in the order it gathered them. The program's files
/// carry the sources the spans name; the library's files publish nothing.
fn failure_from(loaded: &LoadedProgram, gathered: Vec<Diagnostic>) -> BuildFailure {
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
fn failure(errors: Vec<LoadError>, checked: Option<(LoadedProgram, Diagnostic)>) -> BuildFailure {
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compiler::Severity;
    use crate::compiler::{OverlayOnDisk, VirtualSource};
    use crate::syntax::Span;

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
                None,
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
            None,
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
            None,
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
    fn a_clean_file_still_reports_its_type_error() {
        let diagnostics = analyze_file(
            Path::new("/virtual/main.kiru"),
            &VirtualSource::new(&[("main.kiru", "fn main() { let x: int = \"t\"; };")]),
            None,
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
            Some(&root),
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
