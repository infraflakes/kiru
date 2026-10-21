//! Shared test fixtures: build, and read a program through the same facade
//! the binaries use.
//!
//! Every compiler and runtime test drives the same build through a virtual
//! tree, so the fixture is the facade and no test writes a directory.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::compiler::model::{DeclarationId, Program};
use crate::compiler::{BuildFailure, ProgramSource, build};

/// A program tree that stands for files no walk can reach: the tests write
/// one, keyed by the paths relative to a root that names no directory.
#[derive(Debug, Default)]
pub(crate) struct VirtualSource {
    root: PathBuf,
    files: HashMap<PathBuf, String>,
}

impl VirtualSource {
    /// The tree of `files`, whose paths are relative to the virtual root.
    pub(crate) fn new(files: &[(&str, &str)]) -> Self {
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

/// Build one source file, returning the checked program.
pub(crate) fn checked_program(source: &str) -> Program {
    checked_files(&[("main.kiru", source)], "main.kiru").expect("the program checks")
}

/// Build one source file, returning the checked program and the entry it
/// starts from.
pub(crate) fn checked_program_and_entry(source: &str) -> (Program, DeclarationId) {
    let built = build(
        Path::new("main.kiru"),
        &VirtualSource::new(&[("main.kiru", source)]),
    )
    .expect("the program checks");
    (built.program, built.entry)
}

/// Build a set of named files with the given entry, returning the checked
/// program or the first diagnostic message.
pub(crate) fn checked_files(files: &[(&str, &str)], entry: &str) -> Result<Program, String> {
    match build(Path::new(entry), &VirtualSource::new(files)) {
        Ok(built) => Ok(built.program),
        Err(failure) => Err(first_message(&failure)),
    }
}

/// The first diagnostic message the failure names, as the older message-only
/// fixtures expect.
fn first_message(failure: &BuildFailure) -> String {
    failure
        .diagnostics()
        .iter()
        .find_map(|file| file.diagnostics.first())
        .map(|diagnostic| diagnostic.message.clone())
        .unwrap_or_else(|| failure.rendered().to_owned())
}
