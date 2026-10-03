use super::prune_declarations::prune_unreachable;
use crate::compiler::{Program, Value};
use crate::compiler::{load_files, resolve_names, validate_program};
use crate::runtime::Runtime;

/// Load, link, and check one source file.
fn checked(source: &str) -> Program {
    let directory = tempfile::tempdir().expect("temp dir");
    let path = directory.path().join("main.kiru");
    std::fs::write(&path, source).expect("write file");
    let mut loaded = load_files::load_files(&path).expect("loads");
    let mut program = resolve_names::resolve_names(&mut loaded).expect("links");
    validate_program::validate_program(&mut program).expect("checks");
    program
}

/// The names of the declarations a program still carries.
fn retained_names(program: &Program) -> Vec<&str> {
    program
        .declarations
        .iter()
        .map(|declaration| declaration.name.as_str())
        .collect()
}

#[test]
fn drops_std_the_entry_never_reaches() {
    let mut program = checked("fn main() { std::print(\"x\"); };");
    prune_unreachable(&mut program);
    let names = retained_names(&program);
    assert!(names.contains(&"print"), "print is reachable: {names:?}");
    assert!(
        !names.contains(&"eprint"),
        "an unreached std helper must be dropped: {names:?}"
    );
}

#[test]
fn drops_unreachable_user_declarations() {
    let mut program = checked(
        "fn unused() { return(\"\"); };\n\
         txt dead = \"x\";\n\
         rec dead_record = { key = \"v\" };\n\
         fn main() {};",
    );
    prune_unreachable(&mut program);
    assert_eq!(retained_names(&program), vec!["main"]);
}

#[test]
fn retains_transitive_calls() {
    let mut program = checked(
        "fn leaf() { return(\"\"); };\n\
         fn middle() { return(leaf()); };\n\
         fn helper() { middle(); };\n\
         fn main() { helper(); };",
    );
    prune_unreachable(&mut program);
    let names = retained_names(&program);
    assert!(names.contains(&"main"));
    assert!(names.contains(&"helper"));
    assert!(names.contains(&"middle"));
    assert!(names.contains(&"leaf"));
}

#[test]
fn retains_module_values_the_entry_reads() {
    let mut program = checked(
        "txt greeting = \"hi\";\n\
         rec backend = { dir = \"/x\" };\n\
         fn read(rec repo) { return(repo.dir); };\n\
         fn main() { std::print(greeting); std::print(read(backend)); };",
    );
    prune_unreachable(&mut program);
    let names = retained_names(&program);
    assert!(names.contains(&"greeting"));
    assert!(names.contains(&"backend"));
    assert!(names.contains(&"read"));
}

#[test]
fn a_retained_program_still_checks() {
    let mut program = checked(
        "rec backend = { dir = \"/x\" };\n\
         fn read(rec repo) { return(repo.dir); };\n\
         fn main() { std::print(read(backend)); };",
    );
    prune_unreachable(&mut program);
    validate_program::validate_program(&mut program).expect("the retained program still checks");
}

#[test]
fn a_retained_program_still_runs() {
    let mut program = checked(
        "fn answer() { return(\"42\"); };\n\
         fn main() { answer(); };",
    );
    prune_unreachable(&mut program);
    let runtime = Runtime::for_testing(program);
    let value = runtime
        .call_root("answer", Vec::new())
        .expect("the retained function runs");
    assert_eq!(value, Value::Text("42".to_owned()));
}

#[test]
fn retained_module_values_still_evaluate() {
    let mut program = checked(
        "txt base = \"a\";\n\
         fn f() { return(base + \"b\"); };\n\
         fn main() { f(); };",
    );
    prune_unreachable(&mut program);
    let runtime = Runtime::for_testing(program);
    let value = runtime
        .call_root("f", Vec::new())
        .expect("the retained module value evaluates");
    assert_eq!(value, Value::Text("ab".to_owned()));
}

#[test]
fn retention_shrinks_the_payload() {
    let mut program = checked("fn main() { std::print(\"x\"); };");
    let before = postcard::to_allocvec(&program).expect("serializes").len();
    prune_unreachable(&mut program);
    let after = postcard::to_allocvec(&program).expect("serializes").len();
    assert!(
        after < before,
        "retention must shrink the payload: {before} bytes before, {after} after"
    );
    eprintln!("payload: {before} bytes before retention, {after} after");
}
