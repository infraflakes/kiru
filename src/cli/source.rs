//! The disk as the compiler's program source.
//!
//! This is where a compile's files enter: the command line supplies this
//! source, so the compiler engine itself never names the disk. The root is
//! the manifest's, already settled; this source only walks it and reads.

use std::path::{Path, PathBuf};

use crate::compiler::ProgramSource;

/// The program tree on the disk, as `kc` reads it.
pub(crate) struct DiskSource;

impl ProgramSource for DiskSource {
    fn collect(&self, tree_root: &Path) -> std::io::Result<Vec<PathBuf>> {
        collect_kiru_files(tree_root)
    }

    fn read(&self, path: &Path) -> std::io::Result<String> {
        std::fs::read_to_string(path)
    }
}

/// Every `.kiru` file under `directory`, recursively. The disk source's
/// program tree is this walk; the loader reads what it names.
fn collect_kiru_files(directory: &Path) -> std::io::Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    for entry in std::fs::read_dir(directory)? {
        let path = entry?.path();
        if path.is_dir() {
            files.extend(collect_kiru_files(&path)?);
        } else if path
            .extension()
            .is_some_and(|extension| extension == "kiru")
        {
            files.push(path);
        }
    }
    Ok(files)
}
