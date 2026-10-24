//! Pruning: reachability and the compacted graph.

use std::collections::HashSet;

use super::declarations::prune_unreachable;
use crate::compiler::model::{DeclarationId, Facts, Program};
use crate::compiler::{
    checked_program_and_entry, lower_bytecode, mark_reachable, validate_program,
};
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

/// A checked program, its facts, its entry, and the declarations the entry
/// reaches.
fn checked(source: &str) -> (Program, Facts, DeclarationId, HashSet<DeclarationId>) {
    let (program, facts, entry) = checked_program_and_entry(source);
    let reachable = mark_reachable(&program, &facts, entry);
    (program, facts, entry, reachable)
}

#[test]
fn drops_std_the_entry_never_reaches() {
    let (mut program, mut facts, entry, reachable) =
        checked("use std;\nuse std::text;\nuse std::dynamic;\nfn main() { std::print(\"x\"); };");
    prune_unreachable(&mut program, &mut facts, entry, &reachable);
    let names = retained_names(&program);
    assert!(names.contains(&"print"), "print is reachable: {names:?}");
    assert!(
        !names.contains(&"fail"),
        "an unreached std helper must be dropped: {names:?}"
    );
}

#[test]
fn drops_unreachable_user_declarations() {
    let (mut program, mut facts, entry, reachable) = checked(
        "use std;\nuse std::text;\nuse std::dynamic;\nfn unused() -> txt { return \"\"; };\n\
         let dead: txt = \"x\";\n\
         rec Dead { key: txt };\n\
         let dead_record: Dead = { key = \"v\" };\n\
         fn main() {};",
    );
    prune_unreachable(&mut program, &mut facts, entry, &reachable);
    assert_eq!(retained_names(&program), vec!["main"]);
}

#[test]
fn retains_transitive_calls() {
    let (mut program, mut facts, entry, reachable) = checked(
        "use std;\nuse std::text;\nuse std::dynamic;\nfn leaf() -> txt { return \"\"; };\n\
         fn middle() -> txt { return leaf(); };\n\
         fn helper() { middle(); };\n\
         fn main() { helper(); };",
    );
    prune_unreachable(&mut program, &mut facts, entry, &reachable);
    let names = retained_names(&program);
    assert!(names.contains(&"main"));
    assert!(names.contains(&"helper"));
    assert!(names.contains(&"middle"));
    assert!(names.contains(&"leaf"));
}

#[test]
fn retains_module_values_the_entry_reads() {
    let (mut program, mut facts, entry, reachable) = checked(
        "use std;\nuse std::text;\nuse std::dynamic;\nlet greeting: txt = \"hi\";\n\
         rec Repo { dir: txt };\n\
         let backend: Repo = { dir = \"/x\" };\n\
         fn read(repo: Repo) -> txt { return repo.dir; };\n\
         fn main() { std::print(greeting); std::print(read(backend)); };",
    );
    prune_unreachable(&mut program, &mut facts, entry, &reachable);
    let names = retained_names(&program);
    assert!(names.contains(&"greeting"));
    assert!(names.contains(&"backend"));
    assert!(names.contains(&"read"));
}

#[test]
fn a_retained_program_still_checks() {
    let (mut program, mut facts, entry, reachable) = checked(
        "use std;\nuse std::text;\nuse std::dynamic;\nrec Repo { dir: txt };\n\
         let backend: Repo = { dir = \"/x\" };\n\
         fn read(repo: Repo) -> txt { return repo.dir; };\n\
         fn main() { std::print(read(backend)); };",
    );
    prune_unreachable(&mut program, &mut facts, entry, &reachable);
    let (_, errors) = validate_program(&program);
    assert!(errors.is_empty(), "the retained program still checks");
}

#[test]
fn retention_shrinks_the_payload() {
    let (mut program, mut facts, entry, reachable) =
        checked("use std;\nuse std::text;\nuse std::dynamic;\nfn main() { std::print(\"x\"); };");
    let before = postcard::to_allocvec(&lower_bytecode(&program, &facts, entry))
        .expect("serializes")
        .len();
    let entry = prune_unreachable(&mut program, &mut facts, entry, &reachable);
    let after = postcard::to_allocvec(&lower_bytecode(&program, &facts, entry))
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
    let (mut program, mut facts, entry, reachable) = checked(
        "use std;\nuse std::text;\nuse std::dynamic;\nuse std::process;\nuse std::Option;\nuse std::dynamic::Map;\nuse std::dynamic::Vec;\nfn answer() -> txt { return \"42\"; };\n\
         fn main() { answer(); };",
    );
    let entry = prune_unreachable(&mut program, &mut facts, entry, &reachable);
    let runtime = Runtime::for_testing(program, facts, entry);
    let value = runtime
        .call_root("answer", Vec::new())
        .expect("the retained function runs");
    assert_eq!(value, Value::Text("42".into()));
}

#[test]
fn retained_module_values_still_evaluate() {
    let (mut program, mut facts, entry, reachable) = checked(
        "use std;\nuse std::text;\nuse std::dynamic;\nuse std::process;\nuse std::Option;\nuse std::dynamic::Map;\nuse std::dynamic::Vec;\nlet base: txt = \"a\";\n\
         fn f() -> txt { return \"@(base)b\"; };\n\
         fn main() { f(); };",
    );
    let entry = prune_unreachable(&mut program, &mut facts, entry, &reachable);
    let runtime = Runtime::for_testing(program, facts, entry);
    let value = runtime
        .call_root("f", Vec::new())
        .expect("the retained module value evaluates");
    assert_eq!(value, Value::Text("ab".into()));
}

/// Reachability from `main` is the rule, not purity: a module value the entry
/// never reaches is dropped even when its initializer has a side effect, and
/// its instructions never run.
#[test]
fn prunes_an_unreached_module_value_with_a_side_effect() {
    let directory = tempfile::tempdir().expect("temp dir");
    let touched = directory.path().join("touched");
    let source = format!(
        "fn touch(path: txt) -> txt {{ std::fs::write(path, \"x\"); return \"done\"; }};\n\
         let dead: txt = touch(\"{}\");\n\
         fn main() {{}};",
        touched.display()
    );
    let (mut program, mut facts, entry, reachable) = checked(&source);
    let entry = prune_unreachable(&mut program, &mut facts, entry, &reachable);
    let names = retained_names(&program);
    assert!(!names.contains(&"dead"), "an unreached value is pruned");
    assert!(!names.contains(&"touch"), "its initializer is pruned too");
    let _runtime = Runtime::for_testing(program, facts, entry);
    assert!(
        !touched.exists(),
        "a pruned initializer's instructions never run"
    );
}

/// A module value reached only from `main` is kept: reachability, again, and
/// its initializer runs at startup.
#[test]
fn keeps_a_module_value_only_main_reaches() {
    let directory = tempfile::tempdir().expect("temp dir");
    let touched = directory.path().join("touched");
    let source = format!(
        "fn touch(path: txt) -> txt {{ std::fs::write(path, \"x\"); return \"done\"; }};\n\
         let kept: txt = touch(\"{}\");\n\
         fn main() {{ std::text::strlen(kept); }};",
        touched.display()
    );
    let (mut program, mut facts, entry, reachable) = checked(&source);
    let entry = prune_unreachable(&mut program, &mut facts, entry, &reachable);
    let names = retained_names(&program);
    assert!(names.contains(&"kept"), "a reached value is kept");
    assert!(names.contains(&"touch"), "its initializer is kept");
    let _runtime = Runtime::for_testing(program, facts, entry);
    assert!(touched.exists(), "a kept initializer runs at startup");
}
