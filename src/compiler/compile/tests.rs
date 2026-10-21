//! The compile process at its own boundary: a file with syntax errors reports
//! every one of them and writes no output.

use std::path::Path;

use crate::cli::DiskSource;

/// Compile a program in a fresh directory and return the error it reported.
fn compile_program(source: &str) -> (String, std::path::PathBuf) {
    let directory = tempfile::tempdir().expect("a temporary directory");
    let entry = directory.path().join("main.kiru");
    std::fs::write(&entry, source).expect("the program file");
    let output = directory.path().join("main");
    let error = super::compile(&entry, Some(&output), Path::new("kc"), &DiskSource)
        .expect_err("the program does not compile");
    (error, output)
}

#[test]
fn every_syntax_error_reports_and_no_output_is_written() {
    let (error, output) =
        compile_program("fn broken() -> {};\nfn kept() {};\nfn also_broken() -> {};\n");
    assert_eq!(
        error
            .matches("expected a type after `->`, found `{`")
            .count(),
        2,
        "both syntax errors print: {error}"
    );
    assert!(!output.exists(), "no output is written");
}

#[test]
fn a_syntax_error_does_not_hide_a_type_error() {
    let (error, output) = compile_program(
        "use std::text;\nfn broken() -> {};\nfn typed() { let wrong: int = \"t\"; };\nfn main() {};\n",
    );
    assert!(
        error.contains("expected a type after `->`, found `{`"),
        "the syntax error prints: {error}"
    );
    assert!(
        error.contains("expected int, found txt"),
        "the type error the recovered tree earned prints too: {error}"
    );
    assert!(
        error.contains("warning: unused use `std::text`"),
        "and so does the recovered file's unused fact: {error}"
    );
    assert!(!output.exists(), "no output is written");
}
