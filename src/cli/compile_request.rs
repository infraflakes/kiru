//! Compiling an entry file into a standalone executable.

use std::path::Path;

use crate::compiler;

use super::source::DiskSource;

/// Run the compile process and print the message on failure. Returns the
/// process exit code.
pub(crate) fn compile(entry: &Path, output: Option<&Path>, executable: &Path) -> i32 {
    match compiler::compile(entry, output, executable, &DiskSource) {
        Ok(()) => 0,
        Err(message) => {
            eprint!("{message}");
            1
        }
    }
}
