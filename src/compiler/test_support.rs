use crate::compiler::CompileError;
use crate::compiler::compile::compile_and_resolve;
use crate::compiler::resolve;
use crate::compiler::types::Config;
use std::path::Path;

/// Compile a `.kiru` file and assert success.  Wraps the public
/// [`compile_and_resolve`] API so tests focus on assertions.
pub(crate) fn compile_full(entry_path: &Path) -> Result<Config, CompileError> {
    compile_and_resolve(entry_path)
}

/// Write a `.kiru` config file into a temporary directory.
pub(crate) fn write_config(dir: &Path, name: &str, content: &str) {
    let path = dir.join(name);
    std::fs::write(&path, content)
        .unwrap_or_else(|e| panic!("failed to write {}: {}", path.display(), e));
}

/// RAII guard that overrides `KIRU_CWD` for the duration of a test and
/// restores the previous value on drop.  Prevents test interference when
/// the parent process (e.g. `kiru` itself) has the env var set.
pub(crate) struct KiruCwdGuard(Option<bool>);
impl KiruCwdGuard {
    /// Opt out of `KIRU_CWD` — project-scope `var shell` tests expect
    /// the project working directory, not the current process directory.
    pub(crate) fn with_project_dir() -> Self {
        KiruCwdGuard(resolve::__test_set_kiru_cwd(Some(false)))
    }
    /// Opt into `KIRU_CWD` — verify the env-var override forces CWD.
    pub(crate) fn with_kiru_cwd() -> Self {
        KiruCwdGuard(resolve::__test_set_kiru_cwd(Some(true)))
    }
}
impl Drop for KiruCwdGuard {
    fn drop(&mut self) {
        resolve::__test_set_kiru_cwd(self.0);
    }
}
