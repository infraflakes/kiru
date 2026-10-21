//! Shared test fixtures: build, and read a program through the same facade
//! the binaries use.
//!
//! Every compiler and runtime test drives the same build through a virtual
//! tree, so the fixture is the facade and no test writes a directory.

use crate::compiler::model::Program;
use crate::compiler::sources::VirtualSource;
use crate::compiler::{BuildFailure, build};

/// Build one source file, returning the checked program.
pub(crate) fn checked_program(source: &str) -> Program {
    checked_files(&[("main.kiru", source)], "main.kiru").expect("the program checks")
}

/// Build a set of named files with the given entry, returning the checked
/// program or the first diagnostic message.
pub(crate) fn checked_files(files: &[(&str, &str)], entry: &str) -> Result<Program, String> {
    match build(std::path::Path::new(entry), &VirtualSource::new(files)) {
        Ok(built) => Ok(built.program),
        Err(failure) => Err(first_message(&failure)),
    }
}

/// The first diagnostic message the failure names, as the older message-only
/// fixtures expect.
fn first_message(failure: &BuildFailure) -> String {
    failure
        .diagnostics()
        .iter()
        .find_map(|file| file.diagnostics.first())
        .map(|diagnostic| diagnostic.message.clone())
        .unwrap_or_else(|| failure.rendered().to_owned())
}
