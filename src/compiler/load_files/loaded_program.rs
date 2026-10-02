//! The loaded program: its files, their origins, and their ordering.

use std::collections::HashSet;
use std::path::PathBuf;

use crate::syntax::File;

/// Where a loaded file came from.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) enum Origin {
    /// A file seeded from the compiler's own sources.
    Embedded,
    /// A file read from the filesystem.
    File,
}

/// A parsed file together with the resolved indexes of its imports.
#[derive(Clone, Debug)]
pub(crate) struct LoadedFile {
    pub(crate) path: PathBuf,
    pub(crate) source: String,
    pub(crate) file: File,
    pub(crate) imports: Vec<usize>,
    pub(crate) origin: Origin,
}

/// The whole program: every loaded file plus the entry index.
#[derive(Clone, Debug)]
pub(crate) struct LoadedProgram {
    pub(crate) entry: usize,
    pub(crate) files: Vec<LoadedFile>,
}

impl LoadedProgram {
    /// File indexes in declaration order: imports first, depth first, then
    /// the importing file.
    pub(crate) fn ordered_files(&self) -> Vec<usize> {
        fn visit(
            files: &[LoadedFile],
            index: usize,
            visited: &mut HashSet<usize>,
            order: &mut Vec<usize>,
        ) {
            if !visited.insert(index) {
                return;
            }
            for &import in &files[index].imports {
                visit(files, import, visited, order);
            }
            order.push(index);
        }

        let mut order = Vec::new();
        let mut visited = HashSet::new();
        visit(&self.files, self.entry, &mut visited, &mut order);
        order
    }
}
