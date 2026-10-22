//! Shared test fixtures: build, and read a program through the same facade
//! the binaries use.
//!
//! Every compiler and runtime test drives the same build through a virtual
//! tree, so the fixture is the facade and no test writes a directory.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::compiler::model::{DeclarationId, Program};
use crate::compiler::{BuildFailure, ProgramSource, build, program_main};

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

    /// The tree root every file is under.
    pub(crate) fn root(&self) -> &Path {
        &self.root
    }
}

impl ProgramSource for VirtualSource {
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
    checked_files(&[("main.kiru", source)]).expect("the program checks")
}

/// Build one source file, returning the checked program and its root `main`.
pub(crate) fn checked_program_and_entry(source: &str) -> (Program, DeclarationId) {
    let source = VirtualSource::new(&[("main.kiru", source)]);
    let built = build(
        source.root(),
        &source,
        &crate::project::WarningPolicy::default(),
    )
    .expect("the program checks");
    assert!(
        built.loaded.syntax.is_empty(),
        "the program parses: {:?}",
        built.loaded.syntax
    );
    let main = program_main(&built.program)
        .expect("the entry is well formed")
        .expect("the program declares a root main");
    (built.program, main)
}

/// Build a set of named files, returning the checked program or the first
/// diagnostic message. The program must declare a root `main`, as a compile
/// does.
pub(crate) fn checked_files(files: &[(&str, &str)]) -> Result<Program, String> {
    let source = VirtualSource::new(files);
    match build(
        source.root(),
        &source,
        &crate::project::WarningPolicy::default(),
    ) {
        // A recovered tree still builds its model, but it is not a program:
        // its syntax errors are the first fact the fixture reports.
        Ok(built) if !built.loaded.syntax.is_empty() => Err(built.loaded.syntax[0].message.clone()),
        Ok(built) => match program_main(&built.program) {
            Ok(Some(_)) => Ok(built.program),
            Ok(None) => Err(crate::compiler::MISSING_MAIN.to_owned()),
            Err(diagnostic) => Err(diagnostic.message),
        },
        Err(failure) => Err(first_message(&failure)),
    }
}

/// The first diagnostic message the failure names, as the older message-only
/// fixtures expect.
fn first_message(failure: &BuildFailure) -> String {
    failure
        .diagnostics()
        .iter()
        .find_map(|file| {
            file.diagnostics.iter().find(|diagnostic| {
                matches!(diagnostic.severity(), crate::compiler::Severity::Error)
            })
        })
        .map(|diagnostic| diagnostic.message.clone())
        .unwrap_or_else(|| failure.rendered().to_owned())
}
