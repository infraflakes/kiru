//! Where a program's files come from.
//!
//! The compiler is parameterized over the source of its program tree: the
//! disk that `kc` reads, the test tree a virtual table stands for, and the
//! editor overlay the language server keeps. The embedded standard library is
//! not a choice: every source carries it, so the three sources differ only in
//! how they locate the entry, name the program tree's files, and read one
//! file's source.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// The source of a program tree: where the entry stands, which files the tree
/// holds, and what one file's source is. Every answer is a plain path or
/// text; the loader owns the namespaces and the parsing.
pub trait ProgramSource {
    /// The target: a file is the entry the loader records, beside the tree
    /// root the program's namespaces derive from; a directory is the tree
    /// itself, with no entry.
    fn locate(&self, target: &Path) -> std::io::Result<(Option<PathBuf>, PathBuf)>;
    /// Every `.kiru` file of the program tree under the tree root.
    fn collect(&self, tree_root: &Path) -> std::io::Result<Vec<PathBuf>>;
    /// The source of one program file.
    fn read(&self, path: &Path) -> std::io::Result<String>;
}

/// The program tree on the disk, as `kc` reads it.
pub struct DiskSource;

impl ProgramSource for DiskSource {
    fn locate(&self, target: &Path) -> std::io::Result<(Option<PathBuf>, PathBuf)> {
        let canonical = target.canonicalize()?;
        if canonical.is_dir() {
            return Ok((None, canonical));
        }
        let tree_root = canonical.parent().unwrap_or(Path::new(".")).to_path_buf();
        Ok((Some(canonical), tree_root))
    }

    fn collect(&self, tree_root: &Path) -> std::io::Result<Vec<PathBuf>> {
        crate::compiler::collect_kiru_files(tree_root)
    }

    fn read(&self, path: &Path) -> std::io::Result<String> {
        std::fs::read_to_string(path)
    }
}

/// The program tree the editor keeps: every file is read from the disk unless
/// the overlay holds a buffer for it. The entry and the tree walk stay on the
/// disk, so a buffer the editor holds for a file under the tree reads through
/// the same tree a compile would walk.
#[derive(Debug, Default)]
pub struct OverlaySource {
    buffers: HashMap<PathBuf, String>,
}

impl OverlaySource {
    pub fn new() -> Self {
        Self::default()
    }

    /// An editor buffer took the place of the file's disk content.
    pub fn open(&mut self, path: PathBuf, source: String) {
        self.buffers.insert(path, source);
    }

    /// The buffer changed: the whole text arrives with every change.
    pub fn change(&mut self, path: PathBuf, source: String) {
        self.open(path, source);
    }

    /// The editor dropped the buffer: the file reads from the disk again.
    pub fn close(&mut self, path: &Path) {
        self.buffers.remove(path);
    }
}

impl ProgramSource for OverlaySource {
    fn locate(&self, target: &Path) -> std::io::Result<(Option<PathBuf>, PathBuf)> {
        DiskSource.locate(target)
    }

    fn collect(&self, tree_root: &Path) -> std::io::Result<Vec<PathBuf>> {
        crate::compiler::collect_kiru_files(tree_root)
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
        std::fs::read_to_string(path)
    }
}

/// A program tree that stands for files no walk can reach: the tests write
/// one, keyed by the paths relative to a root that names no directory.
#[derive(Debug, Default)]
pub struct VirtualSource {
    root: PathBuf,
    files: HashMap<PathBuf, String>,
}

impl VirtualSource {
    /// The tree of `files`, whose paths are relative to the virtual root.
    pub fn new(files: &[(&str, &str)]) -> Self {
        let root = PathBuf::from("/virtual");
        let files = files
            .iter()
            .map(|(relative, source)| (root.join(relative), source.to_string()))
            .collect();
        Self { root, files }
    }
}

impl ProgramSource for VirtualSource {
    fn locate(&self, target: &Path) -> std::io::Result<(Option<PathBuf>, PathBuf)> {
        let entry_path = self.root.join(target);
        if !self.files.contains_key(&entry_path) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("{}", target.display()),
            ));
        }
        Ok((Some(entry_path), self.root.clone()))
    }

    fn collect(&self, tree_root: &Path) -> std::io::Result<Vec<PathBuf>> {
        Ok(self
            .files
            .keys()
            .filter(|path| path.starts_with(tree_root))
            .cloned()
            .collect())
    }

    fn read(&self, path: &Path) -> std::io::Result<String> {
        self.files.get(path).cloned().ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::NotFound, format!("{}", path.display()))
        })
    }
}
