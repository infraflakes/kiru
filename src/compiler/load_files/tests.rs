use std::path::{Path, PathBuf};

use super::loader::embedded::EMBEDDED;
use super::loader::load_files;
use crate::syntax::{self};

fn write(directory: &Path, name: &str, contents: &str) -> PathBuf {
    let path = directory.join(name);
    std::fs::create_dir_all(path.parent().expect("has a parent")).expect("create directory");
    std::fs::write(&path, contents).expect("write file");
    path
}

/// The namespace of the declaration named `name` in the joined item stream.
fn namespace_of(program: &super::LoadedProgram, name: &str) -> Vec<String> {
    program
        .items
        .iter()
        .find(|item| item.declaration.name == name)
        .map(|item| item.namespace.clone())
        .unwrap_or_else(|| panic!("`{name}` is declared"))
}

#[test]
fn embedded_standard_library_parses() {
    for (path, source) in EMBEDDED {
        syntax::parse_file(source)
            .unwrap_or_else(|error| panic!("{path} must parse: {}", error.message));
    }
}

/// The embedded table mirrors `std/**`, and the loader reads every file in it.
#[test]
fn every_embedded_file_is_loaded() {
    let directory = tempfile::tempdir().expect("temp dir");
    let entry = write(directory.path(), "main.kiru", "fn main() {};\n");
    let program = load_files(&entry).expect("loads");
    let loaded: Vec<&str> = program
        .files
        .iter()
        .filter(|file| matches!(file.origin, crate::compiler::model::Origin::Embedded))
        .map(|file| file.path.to_str().expect("a path"))
        .collect();
    let mut expected: Vec<&str> = EMBEDDED.iter().map(|(path, _)| *path).collect();
    expected.sort();
    let mut loaded = loaded;
    loaded.sort();
    assert_eq!(
        loaded, expected,
        "the embedded table does not mirror `std/**`"
    );
}

#[test]
fn a_file_belongs_to_its_directory_namespace() {
    let directory = tempfile::tempdir().expect("temp dir");
    let entry = write(directory.path(), "main.kiru", "fn main() {};\n");
    write(directory.path(), "task.kiru", "fn task() {};\n");
    write(directory.path(), "sub/worker.kiru", "fn worker() {};\n");

    let program = load_files(&entry).expect("loads");
    assert_eq!(namespace_of(&program, "main"), vec!["root".to_owned()]);
    // A sibling file shares the root namespace; a file in a directory nests.
    assert_eq!(namespace_of(&program, "task"), vec!["root".to_owned()]);
    assert_eq!(
        namespace_of(&program, "worker"),
        vec!["root".to_owned(), "sub".to_owned()]
    );
}

#[test]
fn walks_the_entry_directory_recursively() {
    let directory = tempfile::tempdir().expect("temp dir");
    let entry = write(directory.path(), "main.kiru", "fn main() {};\n");
    write(directory.path(), "a/b/deep.kiru", "fn deep() {};\n");

    let program = load_files(&entry).expect("loads");
    assert_eq!(
        namespace_of(&program, "deep"),
        vec!["root".to_owned(), "a".to_owned(), "b".to_owned()]
    );
}

#[test]
fn reports_parse_error_with_file_and_position() {
    let directory = tempfile::tempdir().expect("temp dir");
    let entry = write(directory.path(), "main.kiru", "fn f( {\n");
    let error = load_files(&entry).expect_err("parse error fails");
    assert!(
        error.render().contains("main.kiru:1:7"),
        "{}",
        error.render()
    );
}
