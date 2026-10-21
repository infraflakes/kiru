//! The editor's buffers over the disk, as the language server's program
//! source.
//!
//! Every file is read from the disk unless the overlay holds a buffer for it.
//! The target and the tree walk stay on the disk, so a buffer the editor holds
//! for a file under the tree reads through the same tree a compile would walk.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::cli::DiskSource;
use crate::compiler::ProgramSource;

/// The program tree the editor keeps: every file is read from the disk unless
/// the overlay holds a buffer for it.
#[derive(Debug, Default)]
pub(crate) struct OverlaySource {
    buffers: HashMap<PathBuf, String>,
}

impl OverlaySource {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// An editor buffer took the place of the file's disk content.
    pub(crate) fn open(&mut self, path: PathBuf, source: String) {
        self.buffers.insert(path, source);
    }
}

impl ProgramSource for OverlaySource {
    fn collect(&self, tree_root: &Path) -> std::io::Result<Vec<PathBuf>> {
        let mut files = DiskSource.collect(tree_root)?;
        // A buffer the editor opened before its file exists is still in the
        // tree: the editor's text is the file until the disk catches up.
        for path in self.buffers.keys() {
            if path.starts_with(tree_root) && !files.contains(path) {
                files.push(path.clone());
            }
        }
        Ok(files)
    }

    fn read(&self, path: &Path) -> std::io::Result<String> {
        if let Some(source) = self.buffers.get(path) {
            return Ok(source.clone());
        }
        // The buffer is keyed by the path the editor named; a walk under a
        // canonical tree root names canonical paths, so a buffer held under a
        // different spelling of the same file is read by its canonical path.
        let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        if let Some(source) = self.buffers.get(&canonical) {
            return Ok(source.clone());
        }
        DiskSource.read(path)
    }
}
