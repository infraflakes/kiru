//! Finding the manifest a file belongs to: the nearest one above it.

use std::path::{Path, PathBuf};

/// The manifest nearest to `path`, if any. A project is the nearest
/// `kiru.toml` above the file, so a file under a nested project belongs to
/// that project and no other.
pub(crate) fn find(path: &Path) -> Option<PathBuf> {
    let start = if path.is_dir() { path } else { path.parent()? };
    start
        .ancestors()
        .map(|directory| directory.join("kiru.toml"))
        .find(|manifest| manifest.is_file())
}
