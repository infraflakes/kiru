use super::prune_declarations::prune_unreachable;
use crate::compiler::{checked_program, lower_bytecode, validate_program};
use crate::model::Program;

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
    let mut program = checked_program("fn main() { std::io::print(\"x\"); };");
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
    let mut program = checked_program(
        "fn unused() -> txt { return(\"\"); };\n\
         txt dead = \"x\";\n\
         rec dead_record = { key = \"v\" };\n\
         fn main() {};",
    );
    prune_unreachable(&mut program);
    assert_eq!(retained_names(&program), vec!["main"]);
}

#[test]
fn retains_transitive_calls() {
    let mut program = checked_program(
        "fn leaf() -> txt { return(\"\"); };\n\
         fn middle() -> txt { return(leaf()); };\n\
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
        "txt greeting = \"hi\";\n\
         rec backend = { dir = \"/x\" };\n\
         fn read(rec repo) -> txt { return(repo.dir); };\n\
         fn main() { std::io::print(greeting); std::io::print(read(backend)); };",
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
        "rec backend = { dir = \"/x\" };\n\
         fn read(rec repo) -> txt { return(repo.dir); };\n\
         fn main() { std::io::print(read(backend)); };",
    );
    prune_unreachable(&mut program);
    validate_program(&mut program).expect("the retained program still checks");
}

#[test]
fn retention_shrinks_the_payload() {
    let mut program = checked_program("fn main() { std::io::print(\"x\"); };");
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
