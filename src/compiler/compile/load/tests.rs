//! Loading: the tree walk, the embedded library, namespaces, and recovery.

use std::path::{Path, PathBuf};

use super::STD_TREE;
use super::embedded::EMBEDDED;
use super::tree::load_files;
use crate::cli::DiskSource;
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
        let parsed = syntax::parse_file(source)
            .unwrap_or_else(|error| panic!("{path} must parse: {}", error.message));
        assert!(
            parsed.errors.is_empty(),
            "{path} must parse: {:?}",
            parsed.errors
        );
    }
}

/// A path under the repository root.
fn repository_path(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(relative)
}

/// Every `.kiru` file under `root`, as a path relative to `root`, sorted.
fn kiru_files_under(root: &Path) -> Vec<String> {
    fn walk(root: &Path, directory: &Path, out: &mut Vec<String>) {
        for entry in std::fs::read_dir(directory).expect("a source directory is readable") {
            let path = entry.expect("a source entry is readable").path();
            if path.is_dir() {
                walk(root, &path, out);
            } else if path
                .extension()
                .is_some_and(|extension| extension == "kiru")
            {
                let relative = path.strip_prefix(root).expect("a file under the walk root");
                out.push(relative.to_string_lossy().into_owned());
            }
        }
    }
    let mut files = Vec::new();
    walk(root, root, &mut files);
    files.sort();
    files
}

/// Every `.kiru` file under `std/`, keyed by its path under the tree.
fn std_directory_files() -> Vec<String> {
    kiru_files_under(&repository_path("std"))
}

/// The generated embedded table mirrors `std/**`, and the loader reads every
/// file in it. The comparison is against the directory, not against the table,
/// so a file is never missing from the image by being missing from a second
/// list.
#[test]
fn the_embedded_table_mirrors_the_std_directory() {
    let expected = std_directory_files();
    let mut embedded: Vec<String> = EMBEDDED
        .iter()
        .map(|(path, _)| (*path).to_owned())
        .collect();
    embedded.sort();
    assert_eq!(
        embedded, expected,
        "the embedded table does not mirror `std/**`"
    );

    let directory = tempfile::tempdir().expect("temp dir");
    let entry = write(directory.path(), "main.kiru", "fn main() {};\n");
    let program = load_files(&entry, &DiskSource).expect("loads");
    let prefix = format!("<{STD_TREE}>/");
    let mut loaded: Vec<String> = program
        .files
        .iter()
        .filter_map(|file| {
            file.path
                .to_string_lossy()
                .strip_prefix(&prefix)
                .map(str::to_owned)
        })
        .collect();
    loaded.sort();
    assert_eq!(
        loaded, expected,
        "the loader does not read every file under `std/**`"
    );
}

#[test]
fn a_file_belongs_to_its_directory_namespace() {
    let directory = tempfile::tempdir().expect("temp dir");
    let entry = write(directory.path(), "main.kiru", "fn main() {};\n");
    write(directory.path(), "task.kiru", "fn task() {};\n");
    write(directory.path(), "sub/worker.kiru", "fn worker() {};\n");

    let program = load_files(&entry, &DiskSource).expect("loads");
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

    let program = load_files(&entry, &DiskSource).expect("loads");
    assert_eq!(
        namespace_of(&program, "deep"),
        vec!["root".to_owned(), "a".to_owned(), "b".to_owned()]
    );
}

#[test]
fn reports_parse_error_with_file_and_position() {
    let directory = tempfile::tempdir().expect("temp dir");
    let entry = write(directory.path(), "main.kiru", "fn f( {\n");
    let program = load_files(&entry, &DiskSource).expect("the recovered file loads");
    let diagnostic = program.syntax.first().expect("one syntax error");
    let source = program
        .files
        .iter()
        .find(|file| file.path == diagnostic.path)
        .expect("the error names a loaded file")
        .source
        .clone();
    assert!(
        diagnostic.render(&source).contains("main.kiru:1:7"),
        "{}",
        diagnostic.render(&source)
    );
}

/// The item keywords a root facade may not declare.
const FORBIDDEN_FACADE_ITEMS: [&str; 7] =
    ["fn", "struct", "enum", "impl", "trait", "const", "static"];

/// The tokens of Rust source with comments and string and character literals
/// removed, so a keyword inside either is not mistaken for an item.
fn rust_tokens(source: &str) -> Vec<String> {
    let characters: Vec<char> = source.chars().collect();
    let mut tokens = Vec::new();
    let mut index = 0;
    while index < characters.len() {
        let character = characters[index];
        let next = characters.get(index + 1).copied();
        if character.is_whitespace() {
            index += 1;
        } else if character == '/' && next == Some('/') {
            while index < characters.len() && characters[index] != '\n' {
                index += 1;
            }
        } else if character == '/' && next == Some('*') {
            let mut depth = 1;
            index += 2;
            while index < characters.len() && depth > 0 {
                if characters[index] == '/' && characters.get(index + 1) == Some(&'*') {
                    depth += 1;
                    index += 2;
                } else if characters[index] == '*' && characters.get(index + 1) == Some(&'/') {
                    depth -= 1;
                    index += 2;
                } else {
                    index += 1;
                }
            }
        } else if character == '"' {
            index += 1;
            while index < characters.len() {
                if characters[index] == '\\' {
                    index += 2;
                } else if characters[index] == '"' {
                    index += 1;
                    break;
                } else {
                    index += 1;
                }
            }
        } else if character == '\'' && characters.get(index + 2) == Some(&'\'') {
            index += 3;
        } else if character.is_alphanumeric() || character == '_' {
            let start = index;
            while index < characters.len()
                && (characters[index].is_alphanumeric() || characters[index] == '_')
            {
                index += 1;
            }
            tokens.push(characters[start..index].iter().collect());
        } else {
            tokens.push(character.to_string());
            index += 1;
        }
    }
    tokens
}

/// The index just past the group that opens at `start`.
fn after_group(tokens: &[String], start: usize, open: &str, close: &str) -> usize {
    let mut depth = 0;
    let mut index = start;
    while let Some(token) = tokens.get(index) {
        if token == open {
            depth += 1;
        } else if token == close {
            depth -= 1;
            if depth == 0 {
                return index + 1;
            }
        }
        index += 1;
    }
    index
}

/// The first item keyword a root facade declares, if any. A facade may hold
/// only `mod` and `use`, with their attributes.
fn facade_item_keyword(source: &str) -> Option<&'static str> {
    let tokens = rust_tokens(source);
    let mut index = 0;
    let mut at_item_start = true;
    while index < tokens.len() {
        if at_item_start {
            while tokens.get(index).is_some_and(|token| token == "#") {
                index += 1;
                if tokens.get(index).is_some_and(|token| token == "!") {
                    index += 1;
                }
                if tokens.get(index).is_some_and(|token| token == "[") {
                    index = after_group(&tokens, index, "[", "]");
                }
            }
            if tokens.get(index).is_some_and(|token| token == "pub") {
                index += 1;
                if tokens.get(index).is_some_and(|token| token == "(") {
                    index = after_group(&tokens, index, "(", ")");
                }
            }
            if tokens
                .get(index)
                .is_some_and(|token| token == "unsafe" || token == "async")
            {
                index += 1;
            }
            if let Some(token) = tokens.get(index)
                && let Some(keyword) = FORBIDDEN_FACADE_ITEMS
                    .iter()
                    .find(|keyword| **keyword == token.as_str())
            {
                return Some(keyword);
            }
            if index >= tokens.len() {
                break;
            }
            at_item_start = false;
        }
        if tokens.get(index).is_some_and(|token| token == ";") {
            at_item_start = true;
        }
        index += 1;
    }
    None
}

/// A root `src/*.rs` file is a facade: it names submodules and re-exports the
/// entry points, while every item lives under `src/<dir>/`. `main.rs` is the
/// binary entry and is exempt.
#[test]
fn root_source_files_are_facades_only() {
    let src = repository_path("src");
    for entry in std::fs::read_dir(&src).expect("the src directory is readable") {
        let path = entry.expect("a src entry is readable").path();
        if path.extension().is_none_or(|extension| extension != "rs") {
            continue;
        }
        let name = path
            .file_name()
            .expect("a source file has a name")
            .to_string_lossy()
            .into_owned();
        if name == "main.rs" {
            continue;
        }
        let source = std::fs::read_to_string(&path).expect("a root source file is readable");
        if let Some(keyword) = facade_item_keyword(&source) {
            panic!(
                "`src/{name}` declares a `{keyword}` item; a root facade may hold only `mod`, `use`, attributes, and comments"
            );
        }
    }
}

/// Every directory under `std/` is a library namespace, so each one carries a
/// `natives.md` listing its natives.
#[test]
fn every_library_namespace_lists_its_natives() {
    let std_root = repository_path("std");
    for entry in std::fs::read_dir(&std_root).expect("the std directory is readable") {
        let path = entry.expect("a std entry is readable").path();
        if !path.is_dir() {
            continue;
        }
        let name = path
            .file_name()
            .expect("a namespace directory has a name")
            .to_string_lossy()
            .into_owned();
        assert!(
            path.join("natives.md").is_file(),
            "`std/{name}` is a library namespace but has no `natives.md`; every directory under `std/` lists its natives there"
        );
    }
}

/// A library file's header names the namespace it belongs to, so a reader
/// knows what a file's declarations resolve against without trusting its
/// arbitrary file name.
#[test]
fn every_library_file_header_names_its_namespace() {
    for relative in std_directory_files() {
        let path = repository_path("std").join(&relative);
        let source = std::fs::read_to_string(&path).expect("a library file is readable");
        let parent = Path::new(&relative)
            .parent()
            .filter(|parent| !parent.as_os_str().is_empty());
        let namespace = match parent {
            Some(parent) => format!("std/{}", parent.to_string_lossy()),
            None => "std".to_owned(),
        };
        let first = source.lines().find(|line| !line.trim().is_empty());
        let header = first.map(str::trim_start).unwrap_or("");
        assert!(
            header.starts_with('#') && header.contains(namespace.as_str()),
            "`std/{relative}` must begin with a `#` header that names its namespace `{namespace}`; found `{header}`"
        );
    }
}

/// A source file that holds only comments and whitespace declares nothing, so
/// it is a leftover rather than a namespace member.
#[test]
fn every_source_file_declares_something() {
    for root in ["std", "pipeline"] {
        let root_path = repository_path(root);
        for relative in kiru_files_under(&root_path) {
            let source = std::fs::read_to_string(root_path.join(&relative))
                .expect("a source file is readable");
            let declares = source.lines().any(|line| {
                let line = line.trim();
                !line.is_empty() && !line.starts_with('#')
            });
            assert!(
                declares,
                "`{root}/{relative}` holds only comments and whitespace; a source file must declare something"
            );
        }
    }
}
