use super::retain;
use crate::compiler::{Program, Value};
use crate::compiler::{check, link, loader};
use crate::runtime::Runtime;

/// Load, link, and check one source file.
fn checked(source: &str) -> Program {
    let directory = tempfile::tempdir().expect("temp dir");
    let path = directory.path().join("main.kiru");
    std::fs::write(&path, source).expect("write file");
    let mut loaded = loader::load(&path).expect("loads");
    let mut program = link::link(&mut loaded).expect("links");
    check::analyze(&mut program).expect("checks");
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
    retain(&mut program);
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
        "fn unused() { return \"\"; };\n\
         txt dead = \"x\";\n\
         rec dead_record = { key = \"v\" };\n\
         fn main() {};",
    );
    retain(&mut program);
    assert_eq!(retained_names(&program), vec!["main"]);
}

#[test]
fn retains_transitive_calls() {
    let mut program = checked(
        "fn leaf() { return \"\"; };\n\
         fn middle() { return leaf(); };\n\
         fn helper() { middle(); };\n\
         fn main() { helper(); };",
    );
    retain(&mut program);
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
         fn read(rec repo) { return repo.dir; };\n\
         fn main() { std::print(greeting); std::print(read(backend)); };",
    );
    retain(&mut program);
    let names = retained_names(&program);
    assert!(names.contains(&"greeting"));
    assert!(names.contains(&"backend"));
    assert!(names.contains(&"read"));
}

#[test]
fn a_retained_program_still_checks() {
    let mut program = checked(
        "rec backend = { dir = \"/x\" };\n\
         fn read(rec repo) { return repo.dir; };\n\
         fn main() { std::print(read(backend)); };",
    );
    retain(&mut program);
    check::analyze(&mut program).expect("the retained program still checks");
}

#[test]
fn a_retained_program_still_runs() {
    let mut program = checked(
        "fn answer() { return \"42\"; };\n\
         fn main() { answer(); };",
    );
    retain(&mut program);
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
         fn f() { return base + \"b\"; };\n\
         fn main() { f(); };",
    );
    retain(&mut program);
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
    retain(&mut program);
    let after = postcard::to_allocvec(&program).expect("serializes").len();
    assert!(
        after < before,
        "retention must shrink the payload: {before} bytes before, {after} after"
    );
    eprintln!("payload: {before} bytes before retention, {after} after");
}
