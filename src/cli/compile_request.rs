//! Compiling an entry file into a standalone executable.

use std::path::Path;

use crate::compiler;

/// Run the compile pipeline and print the message on failure. Returns the
/// process exit code.
pub(crate) fn compile(entry: &Path, output: Option<&Path>, executable: &Path) -> i32 {
    match compiler::compile(entry, output, executable) {
        Ok(()) => 0,
        Err(message) => {
            eprint!("{message}");
            1
        }
    }
}
