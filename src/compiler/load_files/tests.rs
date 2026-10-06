use std::path::{Path, PathBuf};

use super::read_entry_and_imports::{EMBEDDED, load_files};
use crate::syntax::{self, Declaration};

fn write(directory: &Path, name: &str, contents: &str) -> PathBuf {
    let path = directory.join(name);
    std::fs::create_dir_all(path.parent().expect("has a parent")).expect("create directory");
    std::fs::write(&path, contents).expect("write file");
    path
}

/// The declaration names in the joined item stream, in order.
fn names(program: &super::LoadedProgram) -> Vec<&str> {
    program
        .items
        .iter()
        .map(|item| match &item.declaration {
            Declaration::Function(function) => function.name.as_str(),
            Declaration::Binding(binding) => binding.name.as_str(),
        })
        .collect()
}

#[test]
fn embedded_standard_library_parses() {
    for (path, source) in EMBEDDED {
        syntax::parse_file(source)
            .unwrap_or_else(|error| panic!("{path} must parse: {}", error.message));
    }
}

#[test]
fn joins_imports_in_position_order() {
    let directory = tempfile::tempdir().expect("temp dir");
    let entry = write(
        directory.path(),
        "entry.kiru",
        "import \"a.kiru\";\nimport \"b.kiru\";\n",
    );
    write(
        directory.path(),
        "a.kiru",
        "import \"c.kiru\";\nmod m { fn a_fn() {}; };\n",
    );
    write(directory.path(), "c.kiru", "mod m { fn c_fn() {}; };\n");
    write(directory.path(), "b.kiru", "mod m { fn b_fn() {}; };\n");

    let program = load_files(&entry).expect("loads");
    let names = names(&program);
    assert_eq!(&names[names.len() - 3..], &["c_fn", "a_fn", "b_fn"]);
}

#[test]
fn reports_missing_import() {
    let directory = tempfile::tempdir().expect("temp dir");
    let entry = write(directory.path(), "entry.kiru", "import \"gone.kiru\";\n");
    let error = load_files(&entry).expect_err("missing import fails");
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
    let error = load_files(&entry).expect_err("cycle fails");
    assert!(error.message.contains("import cycle"), "{}", error.message);
}

#[test]
fn reports_a_duplicate_import() {
    let directory = tempfile::tempdir().expect("temp dir");
    let entry = write(
        directory.path(),
        "entry.kiru",
        "import \"a.kiru\";\nimport \"b.kiru\";\n",
    );
    write(directory.path(), "a.kiru", "import \"d.kiru\";\n");
    write(directory.path(), "b.kiru", "import \"d.kiru\";\n");
    write(directory.path(), "d.kiru", "mod m { fn d_fn() {}; };\n");
    let error = load_files(&entry).expect_err("a diamond fails");
    assert!(
        error.message.contains("imported more than once"),
        "{}",
        error.message
    );
}

#[test]
fn reports_parse_error_with_file_and_position() {
    let directory = tempfile::tempdir().expect("temp dir");
    let entry = write(directory.path(), "entry.kiru", "fn f( {\n");
    let error = load_files(&entry).expect_err("parse error fails");
    assert!(
        error.render().contains("entry.kiru:1:7"),
        "{}",
        error.render()
    );
}
