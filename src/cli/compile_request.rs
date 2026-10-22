//! Compiling a project into a standalone executable.

use std::path::Path;

use crate::compiler;
use crate::project::Project;

use super::source::DiskSource;

/// Load the manifest, let `-o` override its output, and run the compile
/// process. Returns the process exit code.
pub(crate) fn compile(manifest: &Path, output: Option<&Path>, executable: &Path) -> i32 {
    let mut project = match Project::load(manifest) {
        Ok(project) => project,
        Err(error) => {
            eprintln!("{}: {error}", crate::compiler::DIAGNOSTIC_PREFIX);
            return 1;
        }
    };
    if let Some(output) = output {
        if output
            .extension()
            .is_some_and(|extension| extension == "kiru")
        {
            eprintln!(
                "{}: the output would overwrite a source file",
                crate::compiler::DIAGNOSTIC_PREFIX
            );
            return 1;
        }
        project.output = output.to_path_buf();
    }
    match compiler::compile(&project, executable, &DiskSource) {
        Ok(()) => 0,
        Err(message) => {
            eprint!("{message}");
            1
        }
    }
}
