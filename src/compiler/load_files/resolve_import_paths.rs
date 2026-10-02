//! Turning import paths into canonical filesystem paths.

use std::path::{Path, PathBuf};

/// Resolve one import path against the directory of the importing file. An
/// absolute path is used as written; a relative one is joined to the
/// importing file's directory. The candidate is then canonicalized, so the
/// same file is loaded once however it is named.
pub(super) fn resolve_import(directory: &Path, raw: &str) -> Option<PathBuf> {
    let raw = Path::new(raw);
    let candidate = if raw.is_absolute() {
        raw.to_path_buf()
    } else {
        directory.join(raw)
    };
    candidate.canonicalize().ok()
}
