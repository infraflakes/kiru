use super::prune_declarations::prune_unreachable;
use crate::compiler::model::Program;
use crate::compiler::{checked_program, lower_bytecode, validate_program};
use crate::runtime::Runtime;
use crate::runtime::Value;

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
    let mut program = checked_program(
        "use std;\nuse std::text;\nuse std::dynamic;\nfn main() { std::print(\"x\"); };",
    );
    prune_unreachable(&mut program);
    let names = retained_names(&program);
    assert!(names.contains(&"print"), "print is reachable: {names:?}");
    assert!(
        !names.contains(&"fail"),
        "an unreached std helper must be dropped: {names:?}"
    );
}

#[test]
fn drops_unreachable_user_declarations() {
    let mut program = checked_program(
        "use std;\nuse std::text;\nuse std::dynamic;\nfn unused() -> txt { return \"\"; };\n\
         let dead: txt = \"x\";\n\
         rec Dead { key: txt };\n\
         let dead_record: Dead = { key = \"v\" };\n\
         fn main() {};",
    );
    prune_unreachable(&mut program);
    assert_eq!(retained_names(&program), vec!["main"]);
}

#[test]
fn retains_transitive_calls() {
    let mut program = checked_program(
        "use std;\nuse std::text;\nuse std::dynamic;\nfn leaf() -> txt { return \"\"; };\n\
         fn middle() -> txt { return leaf(); };\n\
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
    let mut program = checked_program(
        "use std;\nuse std::text;\nuse std::dynamic;\nlet greeting: txt = \"hi\";\n\
         rec Repo { dir: txt };\n\
         let backend: Repo = { dir = \"/x\" };\n\
         fn read(repo: Repo) -> txt { return repo.dir; };\n\
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
    let mut program = checked_program(
        "use std;\nuse std::text;\nuse std::dynamic;\nrec Repo { dir: txt };\n\
         let backend: Repo = { dir = \"/x\" };\n\
         fn read(repo: Repo) -> txt { return repo.dir; };\n\
         fn main() { std::print(read(backend)); };",
    );
    prune_unreachable(&mut program);
    validate_program(&mut program).expect("the retained program still checks");
}

#[test]
fn retention_shrinks_the_payload() {
    let mut program = checked_program(
        "use std;\nuse std::text;\nuse std::dynamic;\nfn main() { std::print(\"x\"); };",
    );
    let before = postcard::to_allocvec(&lower_bytecode(&program))
        .expect("serializes")
        .len();
    prune_unreachable(&mut program);
    let after = postcard::to_allocvec(&lower_bytecode(&program))
        .expect("serializes")
        .len();
    assert!(
        after < before,
        "retention must shrink the payload: {before} bytes before, {after} after"
    );
    eprintln!("payload: {before} bytes before retention, {after} after");
}

#[test]
fn a_retained_program_still_runs() {
    let mut program = checked_program(
        "use std;\nuse std::text;\nuse std::dynamic;\nuse std::process;\nuse std::Option;\nuse std::dynamic::Map;\nuse std::dynamic::Vec;\nfn answer() -> txt { return \"42\"; };\n\
         fn main() { answer(); };",
    );
    prune_unreachable(&mut program);
    let runtime = Runtime::for_testing(program);
    let value = runtime
        .call_root("answer", Vec::new())
        .expect("the retained function runs");
    assert_eq!(value, Value::Text("42".into()));
}

#[test]
fn retained_module_values_still_evaluate() {
    let mut program = checked_program(
        "use std;\nuse std::text;\nuse std::dynamic;\nuse std::process;\nuse std::Option;\nuse std::dynamic::Map;\nuse std::dynamic::Vec;\nlet base: txt = \"a\";\n\
         fn f() -> txt { return \"@(base)b\"; };\n\
         fn main() { f(); };",
    );
    prune_unreachable(&mut program);
    let runtime = Runtime::for_testing(program);
    let value = runtime
        .call_root("f", Vec::new())
        .expect("the retained module value evaluates");
    assert_eq!(value, Value::Text("ab".into()));
}
