use crate::compiler::CompileError;
use crate::compiler::compile::compile_and_resolve;
use crate::plan::Plan;
use std::path::Path;

/// Compile a `.kiru` file and assert success, resolving project-body
/// `var shell` commands in the project directory (the default behavior).
/// Wraps the public [`compile_and_resolve`] API so tests focus on assertions.
pub(crate) fn compile_full(entry_path: &Path) -> Result<Plan, CompileError> {
    compile_and_resolve(entry_path, false)
}

/// Compile a `.kiru` file with `force_cwd` set, mirroring the `KIRU_CWD`
/// env var — project-body `var shell` commands run in the current directory.
pub(crate) fn compile_full_with_cwd(
    entry_path: &Path,
    force_cwd: bool,
) -> Result<Plan, CompileError> {
    compile_and_resolve(entry_path, force_cwd)
}

/// Write a `.kiru` config file into a temporary directory.
pub(crate) fn write_config(dir: &Path, name: &str, content: &str) {
    let path = dir.join(name);
    std::fs::write(&path, content)
        .unwrap_or_else(|e| panic!("failed to write {}: {}", path.display(), e));
}
