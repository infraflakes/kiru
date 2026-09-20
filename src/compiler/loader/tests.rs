use std::path::{Path, PathBuf};

use super::loading::{EMBEDDED, load};
use crate::syntax;

fn write(directory: &Path, name: &str, contents: &str) -> PathBuf {
    let path = directory.join(name);
    std::fs::create_dir_all(path.parent().expect("has a parent")).expect("create directory");
    std::fs::write(&path, contents).expect("write file");
    path
}

#[test]
fn embedded_standard_library_parses() {
    for (path, source) in EMBEDDED {
        syntax::parse_file(source)
            .unwrap_or_else(|error| panic!("{path} must parse: {}", error.message));
    }
}

#[test]
fn loads_imports_once_and_depth_first() {
    let directory = tempfile::tempdir().expect("temp dir");
    let entry = write(
        directory.path(),
        "entry.kiru",
        "import \"b.kiru\";\nimport \"a.kiru\";\n",
    );
    write(directory.path(), "a.kiru", "import \"shared.kiru\";\n");
    write(directory.path(), "b.kiru", "import \"shared.kiru\";\n");
    write(directory.path(), "shared.kiru", "txt s = \"s\";\n");

    let program = load(&entry).expect("loads");
    assert_eq!(program.files.len(), EMBEDDED.len() + 4);
    let order = program.ordered_files();
    assert_eq!(order.len(), 4);
    let last = program.files[*order.last().expect("non-empty")]
        .path
        .clone();
    assert!(last.ends_with("entry.kiru"));
    assert_eq!(
        order[0],
        program
            .files
            .iter()
            .position(|file| file.path.ends_with("shared.kiru"))
            .expect("shared")
    );
}

#[test]
fn reports_missing_import() {
    let directory = tempfile::tempdir().expect("temp dir");
    let entry = write(directory.path(), "entry.kiru", "import \"gone.kiru\";\n");
    let error = load(&entry).expect_err("missing import fails");
    assert!(
        error.message.contains("cannot find import gone.kiru"),
        "{}",
        error.message
    );
    assert!(
        error.render().contains("entry.kiru:1:1"),
        "{}",
        error.render()
    );
}

#[test]
fn reports_import_cycle() {
    let directory = tempfile::tempdir().expect("temp dir");
    let entry = write(directory.path(), "entry.kiru", "import \"a.kiru\";\n");
    write(directory.path(), "a.kiru", "import \"entry.kiru\";\n");
    let error = load(&entry).expect_err("cycle fails");
    assert!(error.message.contains("import cycle"), "{}", error.message);
}

#[test]
fn reports_parse_error_with_file_and_position() {
    let directory = tempfile::tempdir().expect("temp dir");
    let entry = write(directory.path(), "entry.kiru", "fn f( {\n");
    let error = load(&entry).expect_err("parse error fails");
    assert!(
        error.render().contains("entry.kiru:1:7"),
        "{}",
        error.render()
    );
}
