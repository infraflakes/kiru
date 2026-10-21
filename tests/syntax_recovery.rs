//! The command line reports every syntax error a file holds and writes no
//! output when it refuses the build.

use std::process::Command;

#[test]
fn two_syntax_errors_are_both_reported_and_no_output_is_written() {
    let directory = tempfile::tempdir().expect("temp dir");
    let entry = directory.path().join("main.kiru");
    std::fs::write(
        &entry,
        "fn broken() -> {};\nfn kept() {};\nfn also_broken() -> {};\n",
    )
    .expect("write entry");
    let output = directory.path().join("main");
    let run = Command::new(env!("CARGO_BIN_EXE_kc"))
        .arg(&entry)
        .arg("-o")
        .arg(&output)
        .output()
        .expect("the compiler runs");
    let stderr = String::from_utf8_lossy(&run.stderr);
    assert!(!run.status.success(), "the build fails: {stderr}");
    assert_eq!(
        stderr
            .matches("expected a type after `->`, found `{`")
            .count(),
        2,
        "both syntax errors print: {stderr}"
    );
    assert!(!output.exists(), "no output is written");
}

#[test]
fn a_syntax_error_does_not_hide_a_type_error() {
    let directory = tempfile::tempdir().expect("temp dir");
    let entry = directory.path().join("main.kiru");
    std::fs::write(
        &entry,
        "use std::text;\nfn broken() -> {};\nfn typed() { let wrong: int = \"t\"; };\nfn main() {};\n",
    )
    .expect("write entry");
    let output = directory.path().join("main");
    let run = Command::new(env!("CARGO_BIN_EXE_kc"))
        .arg(&entry)
        .arg("-o")
        .arg(&output)
        .output()
        .expect("the compiler runs");
    let stderr = String::from_utf8_lossy(&run.stderr);
    assert!(!run.status.success(), "the build fails: {stderr}");
    assert!(
        stderr.contains("expected a type after `->`, found `{`"),
        "the syntax error prints: {stderr}"
    );
    assert!(
        stderr.contains("expected int, found txt"),
        "the type error the recovered tree earned prints too: {stderr}"
    );
    assert!(
        stderr.contains("warning: unused use `std::text`"),
        "and so does the recovered file's unused fact: {stderr}"
    );
    assert!(!output.exists(), "no output is written");
}
