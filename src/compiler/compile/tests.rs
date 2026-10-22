//! The compile process at its own boundary: a project with syntax errors
//! reports every one of them and writes no output.

use std::path::{Path, PathBuf};

use crate::cli::DiskSource;
use crate::project::Project;

/// Compile a program in a fresh project and return the error it reported and
/// the output path it would have written.
fn compile_program(source: &str) -> (String, PathBuf) {
    let directory = tempfile::tempdir().expect("a temporary directory");
    std::fs::write(
        directory.path().join("kiru.toml"),
        "[project]\nroot = \".\"\nname = \"program\"\n",
    )
    .expect("the manifest");
    std::fs::write(directory.path().join("main.kiru"), source).expect("the program file");
    let project =
        Project::load(&directory.path().join("kiru.toml")).expect("the manifest is valid");
    let output = project.output.clone();
    let error = super::compile(&project, Path::new("kc"), &DiskSource)
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

#[test]
fn a_project_without_a_root_main_names_the_manifest() {
    let directory = tempfile::tempdir().expect("a temporary directory");
    std::fs::write(
        directory.path().join("kiru.toml"),
        "[project]\nroot = \".\"\nname = \"program\"\n",
    )
    .expect("the manifest");
    std::fs::write(directory.path().join("main.kiru"), "fn helper() {};\n")
        .expect("the program file");
    let project =
        Project::load(&directory.path().join("kiru.toml")).expect("the manifest is valid");
    let error = super::compile(&project, Path::new("kc"), &DiskSource)
        .expect_err("the program has no entry");
    assert!(
        error.contains("the program has no root `main` function"),
        "{error}"
    );
    assert!(
        error.contains("kiru.toml"),
        "the fact names the project's manifest: {error}"
    );
}
