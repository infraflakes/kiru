//! The standard library's own tests.
//!
//! The standard library is kiru source, so its behavior is tested by kiru
//! programs, not by Rust. Every `tests/stdlib/pass/*.kiru` file is a program
//! that checks the library and stops the run with `std::fail` on the first
//! failure; every `tests/stdlib/fail/*.kiru` file must stop the run. The
//! harness compiles each with the shipped compiler, runs it, and reports the
//! program's own message when it fails.

use std::path::{Path, PathBuf};
use std::process::Command;

/// The compiler the harness drives, built by cargo for this test.
const COMPILER: &str = env!("CARGO_BIN_EXE_kc");

#[test]
fn the_standard_library_passes_its_own_tests() {
    run_directory(&directory("pass"), true);
    run_directory(&directory("fail"), false);
}

fn directory(name: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests/stdlib")
        .join(name)
}

/// Compile and run every program in `directory`, requiring each to succeed (or
/// to fail, when `expect_success` is false).
fn run_directory(directory: &Path, expect_success: bool) {
    let mut files = sources(directory);
    files.sort();
    assert!(
        !files.is_empty(),
        "no kiru tests under {}",
        directory.display()
    );
    let output = tempfile::tempdir().expect("a temp directory");
    for file in files {
        let binary = output.path().join(file.file_stem().expect("a file stem"));
        let compile = Command::new(COMPILER)
            .arg(&file)
            .arg("-o")
            .arg(&binary)
            .output()
            .expect("run the compiler");
        assert!(
            compile.status.success(),
            "{} failed to compile:\n{}",
            file.display(),
            String::from_utf8_lossy(&compile.stderr)
        );
        let run = Command::new(&binary)
            .output()
            .expect("run the test program");
        let report = format!(
            "{}\n{}{}",
            file.display(),
            String::from_utf8_lossy(&run.stdout),
            String::from_utf8_lossy(&run.stderr)
        );
        if expect_success {
            assert!(run.status.success(), "a stdlib test failed: {report}");
        } else {
            assert!(!run.status.success(), "a failing test passed: {report}");
        }
    }
}

/// Every `.kiru` file directly inside `directory`, in directory order.
fn sources(directory: &Path) -> Vec<PathBuf> {
    std::fs::read_dir(directory)
        .unwrap_or_else(|error| panic!("read {}: {error}", directory.display()))
        .map(|entry| entry.expect("a directory entry").path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "kiru")
        })
        .collect()
}
