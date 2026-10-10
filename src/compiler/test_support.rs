//! Shared test fixtures: load, link, and check a program from source.
//!
//! Every compiler and runtime test drives the same pipeline, so the fixture
//! lives here once instead of being rebuilt per test module.

use crate::compiler::{load_files, resolve_names, validate_program};
use crate::model::Program;

/// Load, link, and check one source file, returning the checked program.
pub(crate) fn checked_program(source: &str) -> Program {
    checked_files(&[("main.kiru", source)], "main.kiru").expect("the program checks")
}

/// Load, link, and check a set of named files with the given entry, returning
/// the checked program or the first diagnostic message.
pub(crate) fn checked_files(files: &[(&str, &str)], entry: &str) -> Result<Program, String> {
    let directory = tempfile::tempdir().expect("temp dir");
    for (name, source) in files {
        std::fs::write(directory.path().join(name), source).expect("write file");
    }
    let loaded = load_files(&directory.path().join(entry)).map_err(|error| error.message)?;
    let mut program = resolve_names(&loaded).map_err(|diagnostic| diagnostic.message)?;
    validate_program(&mut program).map_err(|diagnostic| diagnostic.message)?;
    Ok(program)
}
