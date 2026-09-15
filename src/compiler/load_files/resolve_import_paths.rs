//! Turning import paths into canonical filesystem paths.

use std::path::{Path, PathBuf};

/// Resolve one import path against the directory of the importing file. An
/// absolute path is used as written; a relative one is joined to the
/// importing file's directory. The candidate is then canonicalized, so the
/// same file is loaded once however it is named.
pub(super) fn resolve_import(directory: &Path, import_path: &str) -> Option<PathBuf> {
    let import_path = Path::new(import_path);
    let candidate = if import_path.is_absolute() {
        import_path.to_path_buf()
    } else {
        directory.join(import_path)
    };
    candidate.canonicalize().ok()
}
