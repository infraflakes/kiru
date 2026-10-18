//! Engine tests. They call functions directly instead of running the entry,
//! so no command output is captured. Only the tests that build commands
//! spawn processes.

use crate::compiler::checked_program;
use crate::runtime::Value;

use super::Runtime;

fn runtime(source: &str) -> Runtime {
    Runtime::for_testing(checked_program(source))
}

fn call_text(runtime: &Runtime, name: &str) -> String {
    match runtime
        .call_root(name, Vec::new())
        .expect("the call succeeds")
    {
        Value::Text(text) => text.as_str().to_owned(),
        other => panic!("expected text, found {other:?}"),
    }
}

/// Run every pure evaluation case, collecting all mismatches so one run
/// reports every case that failed.
fn expect_text_results(cases: &[(&str, &str, &str, &str)]) {
    let mut failures = Vec::new();
    for &(name, source, function, expected) in cases {
        let actual = call_text(&runtime(source), function);
        if actual != expected {
            failures.push(format!("{name}: expected `{expected}`, found `{actual}`"));
        }
    }
    assert!(
        failures.is_empty(),
        "{} evaluation case(s) failed:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

fn call_bool(runtime: &Runtime, name: &str) -> bool {
    match runtime
        .call_root(name, Vec::new())
        .expect("the call succeeds")
    {
        Value::Bool(value) => value,
        other => panic!("expected a boolean, found {other:?}"),
    }
}

/// Run every boolean evaluation case.
fn expect_bool_results(cases: &[(&str, &str, &str, bool)]) {
    let mut failures = Vec::new();
    for &(name, source, function, expected) in cases {
        let actual = call_bool(&runtime(source), function);
        if actual != expected {
            failures.push(format!("{name}: expected `{expected}`, found `{actual}`"));
        }
    }
    assert!(
        failures.is_empty(),
        "{} evaluation case(s) failed:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[test]
fn exit_code_reads_the_returned_int() {
    assert_eq!(super::exit_code(&Value::Int(0)), 0);
    assert_eq!(super::exit_code(&Value::Int(42)), 42);
    assert_eq!(super::exit_code(&Value::Int(300)), 255);
    assert_eq!(super::exit_code(&Value::Int(-1)), 0);
    assert_eq!(super::exit_code(&Value::Nothing), 0);
}

#[test]
fn evaluates_expressions() {
    expect_text_results(&[
        (
            "returns a text value",
            "use std;\nuse std::text;\nuse std::dynamic;\nuse std::process;\nuse std::Option;\nuse std::dynamic::Map;\nuse std::dynamic::Vec;\nfn answer() -> txt { return \"42\"; };\nfn main() {};",
            "answer",
            "42",
        ),
        (
            "a present field reads as the value, in order",
            "use std;\nuse std::text;\nuse std::dynamic;\nuse std::process;\nuse std::Option;\nuse std::dynamic::Map;\nuse std::dynamic::Vec;\nrec R { a: txt, b: txt };\nfn f() -> txt { let r: R = { a = \"1\", b = \"2\" }; let value: txt = r.b; return value; };\nfn main() {};",
            "f",
            "2",
        ),
        (
            "module values are evaluated once at startup",
            "use std;\nuse std::text;\nuse std::dynamic;\nuse std::process;\nuse std::Option;\nuse std::dynamic::Map;\nuse std::dynamic::Vec;\nlet base: txt = \"a\";\nlet derived: txt = \"@(base)b\";\nfn f() -> txt { return \"@(derived)@(base)\"; };\nfn main() {};",
            "f",
            "aba",
        ),
    ]);
}

#[test]
fn interpolation_inserts_values() {
    expect_text_results(&[
        (
            "literal and expression parts",
            "use std;\nuse std::text;\nuse std::dynamic;\nuse std::process;\nuse std::Option;\nuse std::dynamic::Map;\nuse std::dynamic::Vec;\nfn f() -> txt { let name: txt = \"world\"; return \"hello @(name)!\"; };\nfn main() {};",
            "f",
            "hello world!",
        ),
        (
            "an expression with a call",
            "use std;\nuse std::text;\nuse std::dynamic;\nuse std::process;\nuse std::Option;\nuse std::dynamic::Map;\nuse std::dynamic::Vec;\nfn f() -> txt { let s: txt = \"abc\"; return \"len=@(std::text::strlen(s))\"; };\nfn main() {};",
            "f",
            "len=3",
        ),
        (
            "an escaped at sign stays literal",
            "use std;\nuse std::text;\nuse std::dynamic;\nuse std::process;\nuse std::Option;\nuse std::dynamic::Map;\nuse std::dynamic::Vec;\nfn f() -> txt { return \"user\\@host\"; };\nfn main() {};",
            "f",
            "user@host",
        ),
    ]);
}

#[test]
fn match_expressions_produce_values() {
    expect_text_results(&[
        (
            "the first matching arm's value",
            "use std;\nuse std::text;\nuse std::dynamic;\nuse std::process;\nuse std::Option;\nuse std::dynamic::Map;\nuse std::dynamic::Vec;\nfn f() -> txt { let s: txt = \"a\"; let label: txt = match s { \"a\" => \"one\"; _ => \"other\"; }; return label; };\nfn main() {};",
            "f",
            "one",
        ),
        (
            "the default arm's value",
            "use std;\nuse std::text;\nuse std::dynamic;\nuse std::process;\nuse std::Option;\nuse std::dynamic::Map;\nuse std::dynamic::Vec;\nfn f() -> txt { let s: txt = \"z\"; let label: txt = match s { \"a\" => \"one\"; _ => \"other\"; }; return label; };\nfn main() {};",
            "f",
            "other",
        ),
        (
            "a record-valued match",
            "use std;\nuse std::text;\nuse std::dynamic;\nuse std::process;\nuse std::Option;\nuse std::dynamic::Map;\nuse std::dynamic::Vec;\nrec R { k: txt };\nfn f() -> txt { let s: txt = \"a\"; let r: R = match s { \"a\" => { k = \"v\" }; _ => { k = \"w\" }; }; return r.k; };\nfn main() {};",
            "f",
            "v",
        ),
    ]);
}

#[test]
fn match_on_records_and_lists_produces_values() {
    expect_text_results(&[
        (
            "a record subject matches a record literal",
            "use std;\nuse std::text;\nuse std::dynamic;\nuse std::process;\nuse std::Option;\nuse std::dynamic::Map;\nuse std::dynamic::Vec;\nrec S { a: txt };\nfn f() -> txt { let s: S = { a = \"1\" }; let label: txt = match s { { a = \"1\" } => \"one\"; _ => \"other\"; }; return label; };\nfn main() {};",
            "f",
            "one",
        ),
        (
            "record fields match in any order",
            "use std;\nuse std::text;\nuse std::dynamic;\nuse std::process;\nuse std::Option;\nuse std::dynamic::Map;\nuse std::dynamic::Vec;\nrec S { a: txt, b: txt };\nfn f() -> txt { let s: S = { a = \"1\", b = \"2\" }; let label: txt = match s { { b = \"2\", a = \"1\" } => \"hit\"; _ => \"miss\"; }; return label; };\nfn main() {};",
            "f",
            "hit",
        ),
        (
            "a Vec subject matches a reference",
            "use std;\nuse std::text;\nuse std::dynamic;\nuse std::process;\nuse std::Option;\nuse std::dynamic::Map;\nuse std::dynamic::Vec;\nfn f() -> txt { let s: Vec<txt> = Vec.new().push(\"a\").push(\"b\"); let p: Vec<txt> = Vec.new().push(\"a\").push(\"b\"); let label: txt = match s { p => \"hit\"; _ => \"miss\"; }; return label; };\nfn main() {};",
            "f",
            "hit",
        ),
        (
            "no record match falls to the default",
            "use std;\nuse std::text;\nuse std::dynamic;\nuse std::process;\nuse std::Option;\nuse std::dynamic::Map;\nuse std::dynamic::Vec;\nrec S { a: txt };\nfn f() -> txt { let s: S = { a = \"1\" }; let label: txt = match s { { a = \"2\" } => \"one\"; _ => \"other\"; }; return label; };\nfn main() {};",
            "f",
            "other",
        ),
        (
            "a record reference is a pattern",
            "use std;\nuse std::text;\nuse std::dynamic;\nuse std::process;\nuse std::Option;\nuse std::dynamic::Map;\nuse std::dynamic::Vec;\nrec S { a: txt };\nfn f() -> txt { let s: S = { a = \"1\" }; let p: S = { a = \"1\" }; let label: txt = match s { p => \"hit\"; _ => \"miss\"; }; return label; };\nfn main() {};",
            "f",
            "hit",
        ),
        (
            "a Vec reference is a pattern",
            "use std;\nuse std::text;\nuse std::dynamic;\nuse std::process;\nuse std::Option;\nuse std::dynamic::Map;\nuse std::dynamic::Vec;\nfn f() -> txt { let s: Vec<txt> = Vec.new().push(\"a\"); let p: Vec<txt> = Vec.new().push(\"a\"); let label: txt = match s { p => \"hit\"; _ => \"miss\"; }; return label; };\nfn main() {};",
            "f",
            "hit",
        ),
    ]);
}

#[test]
fn nominal_types_evaluate() {
    let runtime = runtime(
        "use std;\nuse std::text;\nuse std::dynamic;\nuse std::process;\nuse std::Option;\nuse std::dynamic::Map;\nuse std::dynamic::Vec;\nrec Point { name: txt, x: int };\n\
         fn make() -> Point { return { name = \"p\", x = 1 }; };\n\
         fn name_of(p: Point) -> txt { return p.name; };\n\
         fn bump(p: Point) -> int { p.x = p.x + 1; return p.x; };\n\
         fn main() {};",
    );
    let point = runtime
        .call_root("make", Vec::new())
        .expect("make succeeds");
    assert_eq!(
        runtime
            .call_root("name_of", vec![point.clone()])
            .expect("name_of succeeds"),
        Value::Text("p".into())
    );
    assert_eq!(
        runtime
            .call_root("bump", vec![point])
            .expect("bump succeeds"),
        Value::Int(2)
    );
}

#[test]
fn lists_and_for_evaluate() {
    expect_text_results(&[
        (
            "a Vec joins",
            "use std;\nuse std::text;\nuse std::dynamic;\nuse std::process;\nuse std::Option;\nuse std::dynamic::Map;\nuse std::dynamic::Vec;\nfn f() -> txt { return Vec.new().push(\"a\").push(\"b\").join(\"-\"); };\nfn main() {};",
            "f",
            "a-b",
        ),
        (
            "split then join",
            "use std;\nuse std::text;\nuse std::dynamic;\nuse std::process;\nuse std::Option;\nuse std::dynamic::Map;\nuse std::dynamic::Vec;\nfn f() -> txt { return \"a,b,c\".split(\",\").join(\"-\"); };\nfn main() {};",
            "f",
            "a-b-c",
        ),
        (
            "lines drops a trailing newline",
            "use std;\nuse std::text;\nuse std::dynamic;\nuse std::process;\nuse std::Option;\nuse std::dynamic::Map;\nuse std::dynamic::Vec;\nfn f() -> txt { let parts: Vec<txt> = \"a\\nb\\n\".lines(); let joined: txt = parts.join(\"|\"); return \"@(std::text::strlen(joined))\"; };\nfn main() {};",
            "f",
            "3",
        ),
        (
            "an indexed loop accumulates over a Vec",
            "use std;\nuse std::text;\nuse std::dynamic;\nuse std::process;\nuse std::Option;\nuse std::dynamic::Map;\nuse std::dynamic::Vec;\nfn f() -> txt { let xs: Vec<txt> = Vec.new().push(\"a\").push(\"b\").push(\"c\"); let out: txt = \"\"; let index: int = 0; let count: int = xs.len(); for { match index < count { false => { break; }; true => {}; }; let x: txt = xs.at(index); out = \"@(out)@(x)\"; index = index + 1; }; return out; };\nfn main() {};",
            "f",
            "abc",
        ),
        (
            "break ends the loop",
            "use std;\nuse std::text;\nuse std::dynamic;\nuse std::process;\nuse std::Option;\nuse std::dynamic::Map;\nuse std::dynamic::Vec;\nfn f() -> txt { let xs: Vec<txt> = Vec.new().push(\"a\").push(\"b\").push(\"c\"); let out: txt = \"\"; let index: int = 0; let count: int = xs.len(); for { match index < count { false => { break; }; true => {}; }; let x: txt = xs.at(index); out = \"@(out)@(x)\"; break; }; return out; };\nfn main() {};",
            "f",
            "a",
        ),
        (
            "a match default skips an element",
            "use std;\nuse std::text;\nuse std::dynamic;\nuse std::process;\nuse std::Option;\nuse std::dynamic::Map;\nuse std::dynamic::Vec;\nfn f() -> txt { let xs: Vec<txt> = Vec.new().push(\"a\").push(\"b\").push(\"c\"); let out: txt = \"\"; let index: int = 0; let count: int = xs.len(); for { match index < count { false => { break; }; true => {}; }; let x: txt = xs.at(index); match x { \"b\" => {}; _ => { out = \"@(out)@(x)\"; }; }; index = index + 1; }; return out; };\nfn main() {};",
            "f",
            "ac",
        ),
        (
            "an unbounded for breaks",
            "use std;\nuse std::text;\nuse std::dynamic;\nuse std::process;\nuse std::Option;\nuse std::dynamic::Map;\nuse std::dynamic::Vec;\nfn f() -> txt { let out: txt = \"x\"; for { out = \"y\"; break; }; return out; };\nfn main() {};",
            "f",
            "y",
        ),
        (
            "an empty Vec runs the body zero times",
            "use std;\nuse std::text;\nuse std::dynamic;\nuse std::process;\nuse std::Option;\nuse std::dynamic::Map;\nuse std::dynamic::Vec;\nfn f() -> txt { let xs: Vec<txt> = Vec.new(); let out: txt = \"x\"; let index: int = 0; let count: int = xs.len(); for { match index < count { false => { break; }; true => {}; }; out = \"y\"; index = index + 1; }; return out; };\nfn main() {};",
            "f",
            "x",
        ),
        (
            "a nested loop over a split",
            "use std;\nuse std::text;\nuse std::dynamic;\nuse std::process;\nuse std::Option;\nuse std::dynamic::Map;\nuse std::dynamic::Vec;\nfn f() -> txt { let rows: Vec<txt> = \"1;2;3\".split(\";\"); let last: txt = \"\"; let row_index: int = 0; let row_count: int = rows.len(); for { match row_index < row_count { false => { break; }; true => {}; }; let row: txt = rows.at(row_index); let cells: Vec<txt> = row.split(\";\"); let cell_index: int = 0; let cell_count: int = cells.len(); for { match cell_index < cell_count { false => { break; }; true => {}; }; last = cells.at(cell_index); cell_index = cell_index + 1; }; row_index = row_index + 1; }; return last; };\nfn main() {};",
            "f",
            "3",
        ),
        (
            "a module Vec value is read",
            "use std;\nuse std::text;\nuse std::dynamic;\nuse std::process;\nuse std::Option;\nuse std::dynamic::Map;\nuse std::dynamic::Vec;\nlet xs: Vec<txt> = Vec.new().push(\"a\").push(\"b\");\nfn f() -> txt { return xs.join(\"\"); };\nfn main() {};",
            "f",
            "ab",
        ),
    ]);
}

#[test]
fn typed_lists_evaluate() {
    expect_text_results(&[
        (
            "a Vec of ints sums over a loop",
            "use std;\nuse std::text;\nuse std::dynamic;\nuse std::process;\nuse std::Option;\nuse std::dynamic::Map;\nuse std::dynamic::Vec;\nfn f() -> txt { let xs: Vec<int> = Vec.new().push(1).push(2).push(3); let s: int = 0; let index: int = 0; let count: int = xs.len(); for { match index < count { false => { break; }; true => {}; }; s = s + xs.at(index); index = index + 1; }; return \"@(s)\"; };\nfn main() {};",
            "f",
            "6",
        ),
        (
            "a Vec of declared recs keeps field access in a loop",
            "use std;\nuse std::text;\nuse std::dynamic;\nuse std::process;\nuse std::Option;\nuse std::dynamic::Map;\nuse std::dynamic::Vec;\nrec Task { name: txt, weight: int };\nfn f() -> txt { let a: Task = { name = \"a\", weight = 2 }; let b: Task = { name = \"b\", weight = 3 }; let tasks: Vec<Task> = Vec.new().push(a).push(b); let s: int = 0; let index: int = 0; let count: int = tasks.len(); for { match index < count { false => { break; }; true => {}; }; let t: Task = tasks.at(index); s = s + t.weight; index = index + 1; }; return \"@(s)\"; };\nfn main() {};",
            "f",
            "5",
        ),
        (
            "Vec values compare by element",
            "use std;\nuse std::text;\nuse std::dynamic;\nuse std::process;\nuse std::Option;\nuse std::dynamic::Map;\nuse std::dynamic::Vec;\nfn f() -> txt { let xs: Vec<int> = Vec.new().push(1).push(2); let ys: Vec<int> = Vec.new().push(1).push(2); let verdict: txt = match xs { ys => \"same\"; _ => \"diff\"; }; return verdict; };\nfn main() {};",
            "f",
            "same",
        ),
    ]);
}

#[test]
fn vec_access_evaluates() {
    expect_text_results(&[
        (
            "an element by index",
            "use std;\nuse std::text;\nuse std::dynamic;\nuse std::process;\nuse std::Option;\nuse std::dynamic::Map;\nuse std::dynamic::Vec;\nfn f() -> txt { let xs: Vec<int> = Vec.new().push(10).push(20).push(30); return \"@(xs.at(1))\"; };\nfn main() {};",
            "f",
            "20",
        ),
        (
            "field access through an element",
            "use std;\nuse std::text;\nuse std::dynamic;\nuse std::process;\nuse std::Option;\nuse std::dynamic::Map;\nuse std::dynamic::Vec;\nrec T { n: txt };\nfn f() -> txt { let a: T = { n = \"a\" }; let b: T = { n = \"b\" }; let xs: Vec<T> = Vec.new().push(a).push(b); return xs.at(1).n; };\nfn main() {};",
            "f",
            "b",
        ),
        (
            "a variable index in a loop",
            "use std;\nuse std::text;\nuse std::dynamic;\nuse std::process;\nuse std::Option;\nuse std::dynamic::Map;\nuse std::dynamic::Vec;\nfn f() -> txt { let xs: Vec<txt> = Vec.new().push(\"a\").push(\"b\").push(\"c\"); let out: txt = \"\"; let i: int = 0; for { out = \"@(out)@(xs.at(i))\"; i = i + 1; match i { 3 => { break; }; _ => {}; }; }; return out; };\nfn main() {};",
            "f",
            "abc",
        ),
    ]);
}

#[test]
fn enum_match_evaluates() {
    expect_text_results(&[
        (
            "a variant payload is bound in the arm",
            "use std;\nuse std::text;\nuse std::dynamic;\nuse std::process;\nuse std::Option;\nuse std::dynamic::Map;\nuse std::dynamic::Vec;\nenum Shape { Dot, Circle(int) };\nfn f() -> txt { let s: Shape = Shape.Circle(4); let label: txt = match s { Shape.Circle(r) => \"@(r * r)\"; Shape.Dot => \"0\"; }; return label; };\nfn main() {};",
            "f",
            "16",
        ),
        (
            "a payloadless variant matches by tag",
            "use std;\nuse std::text;\nuse std::dynamic;\nuse std::process;\nuse std::Option;\nuse std::dynamic::Map;\nuse std::dynamic::Vec;\nenum Color { Red, Green, Blue };\nfn f() -> txt { let c: Color = Color.Green; let label: txt = match c { Color.Red => \"r\"; Color.Green => \"g\"; Color.Blue => \"b\"; }; return label; };\nfn main() {};",
            "f",
            "g",
        ),
        (
            "a variant pattern ignores a payload with underscore",
            "use std;\nuse std::text;\nuse std::dynamic;\nuse std::process;\nuse std::Option;\nuse std::dynamic::Map;\nuse std::dynamic::Vec;\nenum Shape { Circle(int), Rect(int, int) };\nfn f() -> txt { let s: Shape = Shape.Rect(2, 3); let label: txt = match s { Shape.Circle(_) => \"circle\"; Shape.Rect(w, _) => \"@(w)\"; }; return label; };\nfn main() {};",
            "f",
            "2",
        ),
        (
            "the default arm takes the rest",
            "use std;\nuse std::text;\nuse std::dynamic;\nuse std::process;\nuse std::Option;\nuse std::dynamic::Map;\nuse std::dynamic::Vec;\nenum Color { Red, Green, Blue };\nfn f() -> txt { let c: Color = Color.Blue; let label: txt = match c { Color.Red => \"r\"; _ => \"other\"; }; return label; };\nfn main() {};",
            "f",
            "other",
        ),
    ]);
}

#[test]
fn generic_types_evaluate() {
    expect_text_results(&[
        (
            "a generic rec substitutes its fields",
            "use std;\nuse std::text;\nuse std::dynamic;\nuse std::process;\nuse std::Option;\nuse std::dynamic::Map;\nuse std::dynamic::Vec;\nrec Pair<A, B> { first: A, second: B };\nfn f() -> txt { let p: Pair<int, txt> = { first = 7, second = \"z\" }; return \"@(p.first)@(p.second)\"; };\nfn main() {};",
            "f",
            "7z",
        ),
        (
            "a generic enum binds a substituted payload",
            "use std;\nuse std::text;\nuse std::dynamic;\nuse std::process;\nuse std::Option;\nuse std::dynamic::Map;\nuse std::dynamic::Vec;\nenum Option<T> { some(T), none };\nfn f() -> txt { let o: Option<int> = Option.some(9); let label: txt = match o { Option.some(x) => \"@(x)\"; Option.none => \"none\"; }; return label; };\nfn main() {};",
            "f",
            "9",
        ),
    ]);
}

#[test]
fn generic_functions_evaluate() {
    expect_text_results(&[
        (
            "identity infers from the argument",
            "use std;\nuse std::text;\nuse std::dynamic;\nuse std::process;\nuse std::Option;\nuse std::dynamic::Map;\nuse std::dynamic::Vec;\nfn id<T>(x: T) -> T { return x; };\nfn f() -> txt { return \"@(id(5))\"; };\nfn main() {};",
            "f",
            "5",
        ),
        (
            "a generic length over an inferred Vec",
            "use std;\nuse std::text;\nuse std::dynamic;\nuse std::process;\nuse std::Option;\nuse std::dynamic::Map;\nuse std::dynamic::Vec;\nfn length<T>(xs: Vec<T>) -> int { return xs.len(); };\nfn f() -> txt { let xs: Vec<int> = Vec.new().push(1).push(2).push(3); return \"@(length(xs))\"; };\nfn main() {};",
            "f",
            "3",
        ),
        (
            "inference flows through a generic call",
            "use std;\nuse std::text;\nuse std::dynamic;\nuse std::process;\nuse std::Option;\nuse std::dynamic::Map;\nuse std::dynamic::Vec;\nfn id<T>(x: T) -> T { return x; };\nfn wrap<T>(x: T) -> Vec<T> { let out: Vec<T> = Vec.new(); return out.push(x); };\nfn f() -> txt { let xs: Vec<txt> = wrap(id(\"hi\")); return xs.at(0); };\nfn main() {};",
            "f",
            "hi",
        ),
    ]);
}

#[test]
fn vec_mutation_evaluates() {
    expect_text_results(&[
        (
            "replace replaces an element",
            "use std;\nuse std::text;\nuse std::dynamic;\nuse std::process;\nuse std::Option;\nuse std::dynamic::Map;\nuse std::dynamic::Vec;\nfn f() -> txt { let xs: Vec<txt> = Vec.new().push(\"a\").push(\"b\"); xs = xs.replace(1, \"c\"); return xs.join(\"\"); };\nfn main() {};",
            "f",
            "ac",
        ),
        (
            "push appends",
            "use std;\nuse std::text;\nuse std::dynamic;\nuse std::process;\nuse std::Option;\nuse std::dynamic::Map;\nuse std::dynamic::Vec;\nfn f() -> txt { let xs: Vec<int> = Vec.new(); xs = xs.push(1); xs = xs.push(2); return \"@(xs.at(0))@(xs.at(1))\"; };\nfn main() {};",
            "f",
            "12",
        ),
        (
            "a loop builds a Vec",
            "use std;\nuse std::text;\nuse std::dynamic;\nuse std::process;\nuse std::Option;\nuse std::dynamic::Map;\nuse std::dynamic::Vec;\nfn f() -> txt { let xs: Vec<int> = Vec.new(); let i: int = 0; for { xs = xs.push(i * i); i = i + 1; match i { 4 => { break; }; _ => {}; }; }; return \"@(xs.at(3))\"; };\nfn main() {};",
            "f",
            "9",
        ),
    ]);
}

#[test]
fn function_values_evaluate() {
    expect_text_results(&[
        (
            "a function value called indirectly",
            "use std;\nuse std::text;\nuse std::dynamic;\nuse std::process;\nuse std::Option;\nuse std::dynamic::Map;\nuse std::dynamic::Vec;\nfn double(x: int) -> int { return x * 2; };\nfn apply(f: fn(int) -> int, x: int) -> int { return f(x); };\nfn f() -> txt { return \"@(apply(double, 21))\"; };\nfn main() {};",
            "f",
            "42",
        ),
        (
            "map applies a function to every element",
            "use std;\nuse std::text;\nuse std::dynamic;\nuse std::process;\nuse std::Option;\nuse std::dynamic::Map;\nuse std::dynamic::Vec;\nfn double(x: int) -> int { return x * 2; };\nfn f() -> txt { let xs: Vec<int> = Vec.new().push(1).push(2).push(3); let ys: Vec<int> = xs.map(double); return \"@(ys.at(0)),@(ys.at(1)),@(ys.at(2))\"; };\nfn main() {};",
            "f",
            "2,4,6",
        ),
        (
            "filter keeps the elements a predicate accepts",
            "use std;\nuse std::text;\nuse std::dynamic;\nuse std::process;\nuse std::Option;\nuse std::dynamic::Map;\nuse std::dynamic::Vec;\nfn even(x: int) -> bool { return x % 2 == 0; };\nfn f() -> txt { let xs: Vec<int> = Vec.new().push(1).push(2).push(3).push(4); let ys: Vec<int> = xs.filter(even); return \"@(ys.len())@(ys.at(0))\"; };\nfn main() {};",
            "f",
            "22",
        ),
    ]);
}

#[test]
fn text_comparisons_and_sort_evaluate() {
    expect_text_results(&[
        (
            "text orders lexicographically",
            "use std;\nuse std::text;\nuse std::dynamic;\nuse std::process;\nuse std::Option;\nuse std::dynamic::Map;\nuse std::dynamic::Vec;\nfn f() -> txt { let b: bool = \"a\" < \"b\"; let label: txt = match b { true => \"yes\"; _ => \"no\"; }; return label; };\nfn main() {};",
            "f",
            "yes",
        ),
        (
            "sort orders text ascending",
            "use std;\nuse std::text;\nuse std::dynamic;\nuse std::process;\nuse std::Option;\nuse std::dynamic::Map;\nuse std::dynamic::Vec;\nfn f() -> txt { let xs: Vec<txt> = Vec.new().push(\"c\").push(\"a\").push(\"b\"); let ys: Vec<txt> = std::dynamic::sort(xs); return \"@(ys.at(0))@(ys.at(1))@(ys.at(2))\"; };\nfn main() {};",
            "f",
            "abc",
        ),
        (
            "sort_by orders with a comparator",
            "use std;\nuse std::text;\nuse std::dynamic;\nuse std::process;\nuse std::Option;\nuse std::dynamic::Map;\nuse std::dynamic::Vec;\nfn greater(a: int, b: int) -> bool { return a > b; };\nfn f() -> txt { let xs: Vec<int> = Vec.new().push(1).push(3).push(2); let ys: Vec<int> = xs.sort_by(greater); return \"@(ys.at(0))@(ys.at(1))@(ys.at(2))\"; };\nfn main() {};",
            "f",
            "321",
        ),
    ]);
}

#[test]
fn new_vec_methods_evaluate() {
    expect_text_results(&[
        (
            "insert places an element and shifts the rest up",
            "use std;\nuse std::text;\nuse std::dynamic;\nuse std::process;\nuse std::Option;\nuse std::dynamic::Map;\nuse std::dynamic::Vec;\nfn f() -> txt { let xs: Vec<txt> = Vec.new().push(\"a\").push(\"c\"); xs = xs.insert(1, \"b\"); return xs.join(\"\"); };\nfn main() {};",
            "f",
            "abc",
        ),
        (
            "insert at the length appends",
            "use std;\nuse std::text;\nuse std::dynamic;\nuse std::process;\nuse std::Option;\nuse std::dynamic::Map;\nuse std::dynamic::Vec;\nfn f() -> txt { let xs: Vec<txt> = Vec.new().push(\"a\"); xs = xs.insert(1, \"b\"); return xs.join(\"\"); };\nfn main() {};",
            "f",
            "ab",
        ),
        (
            "remove drops an element and shifts the rest down",
            "use std;\nuse std::text;\nuse std::dynamic;\nuse std::process;\nuse std::Option;\nuse std::dynamic::Map;\nuse std::dynamic::Vec;\nfn f() -> txt { let xs: Vec<txt> = Vec.new().push(\"a\").push(\"b\").push(\"c\"); xs = xs.remove(1); return xs.join(\"\"); };\nfn main() {};",
            "f",
            "ac",
        ),
        (
            "is_empty answers whether there are no elements",
            "use std;\nuse std::text;\nuse std::dynamic;\nuse std::process;\nuse std::Option;\nuse std::dynamic::Map;\nuse std::dynamic::Vec;\nfn f() -> txt { let xs: Vec<int> = Vec.new(); let ys: Vec<int> = Vec.new().push(1); return \"@(xs.is_empty()):@(ys.is_empty())\"; };\nfn main() {};",
            "f",
            "true:false",
        ),
        (
            "clear drops every element",
            "use std;\nuse std::text;\nuse std::dynamic;\nuse std::process;\nuse std::Option;\nuse std::dynamic::Map;\nuse std::dynamic::Vec;\nfn f() -> txt { let xs: Vec<txt> = Vec.new().push(\"a\").push(\"b\"); xs = xs.clear(); return \"@(xs.len())\"; };\nfn main() {};",
            "f",
            "0",
        ),
        (
            "slice keeps the half-open range",
            "use std;\nuse std::text;\nuse std::dynamic;\nuse std::process;\nuse std::Option;\nuse std::dynamic::Map;\nuse std::dynamic::Vec;\nfn f() -> txt { let xs: Vec<txt> = Vec.new().push(\"a\").push(\"b\").push(\"c\").push(\"d\"); let ys: Vec<txt> = xs.slice(1, 3); return ys.join(\"\"); };\nfn main() {};",
            "f",
            "bc",
        ),
        (
            "take keeps the first count",
            "use std;\nuse std::text;\nuse std::dynamic;\nuse std::process;\nuse std::Option;\nuse std::dynamic::Map;\nuse std::dynamic::Vec;\nfn f() -> txt { let xs: Vec<txt> = Vec.new().push(\"a\").push(\"b\").push(\"c\"); let ys: Vec<txt> = xs.take(2); return ys.join(\"\"); };\nfn main() {};",
            "f",
            "ab",
        ),
        (
            "take past the length keeps every element",
            "use std;\nuse std::text;\nuse std::dynamic;\nuse std::process;\nuse std::Option;\nuse std::dynamic::Map;\nuse std::dynamic::Vec;\nfn f() -> txt { let xs: Vec<txt> = Vec.new().push(\"a\").push(\"b\").push(\"c\"); let ys: Vec<txt> = xs.take(9); return ys.join(\"\"); };\nfn main() {};",
            "f",
            "abc",
        ),
        (
            "skip drops the first count",
            "use std;\nuse std::text;\nuse std::dynamic;\nuse std::process;\nuse std::Option;\nuse std::dynamic::Map;\nuse std::dynamic::Vec;\nfn f() -> txt { let xs: Vec<txt> = Vec.new().push(\"a\").push(\"b\").push(\"c\"); let ys: Vec<txt> = xs.skip(1); return ys.join(\"\"); };\nfn main() {};",
            "f",
            "bc",
        ),
        (
            "fold combines from the left with the accumulator first",
            "use std;\nuse std::text;\nuse std::dynamic;\nuse std::process;\nuse std::Option;\nuse std::dynamic::Map;\nuse std::dynamic::Vec;\nfn f() -> txt { let xs: Vec<int> = Vec.new().push(1).push(2).push(3); let total: int = xs.fold(0, fn(acc: int, x: int) -> int { return acc + x; }); return \"@(total)\"; };\nfn main() {};",
            "f",
            "6",
        ),
        (
            "each runs over every element and returns nothing",
            "use std;\nuse std::text;\nuse std::dynamic;\nuse std::process;\nuse std::Option;\nuse std::dynamic::Map;\nuse std::dynamic::Vec;\nfn f() -> txt { let xs: Vec<int> = Vec.new().push(1).push(2).push(3); xs.each(fn(x: int) { }); return \"done\"; };\nfn main() {};",
            "f",
            "done",
        ),
        (
            "count answers how many elements satisfy the predicate",
            "use std;\nuse std::text;\nuse std::dynamic;\nuse std::process;\nuse std::Option;\nuse std::dynamic::Map;\nuse std::dynamic::Vec;\nfn even(x: int) -> bool { return x % 2 == 0; };\nfn f() -> txt { let xs: Vec<int> = Vec.new().push(1).push(2).push(3).push(4); return \"@(xs.count(even))\"; };\nfn main() {};",
            "f",
            "2",
        ),
    ]);
}

#[test]
fn each_calls_apply_for_every_element() {
    // A panicking apply proves the element is visited; an empty vec calls it
    // never, so the run completes.
    let visited = runtime(
        "use std;\nuse std::text;\nuse std::dynamic;\nuse std::process;\nuse std::Option;\nuse std::dynamic::Map;\nuse std::dynamic::Vec;\nfn f() -> txt { let xs: Vec<int> = Vec.new().push(1).push(2).push(3); xs.each(fn(x: int) { match x { 3 => { std::eprint(\"visited\"); }; _ => {}; }; }); return \"ok\"; };\nfn main() {};",
    );
    assert!(visited.call_root("f", Vec::new()).is_err());
    let empty = runtime(
        "use std;\nuse std::text;\nuse std::dynamic;\nuse std::process;\nuse std::Option;\nuse std::dynamic::Map;\nuse std::dynamic::Vec;\nfn f() -> txt { let xs: Vec<int> = Vec.new(); xs.each(fn(x: int) { std::eprint(\"visited\"); }); return \"ok\"; };\nfn main() {};",
    );
    assert_eq!(call_text(&empty, "f"), "ok");
}

#[test]
fn an_out_of_range_insert_fails_the_run() {
    let runtime = runtime(
        "use std;\nuse std::text;\nuse std::dynamic;\nuse std::process;\nuse std::Option;\nuse std::dynamic::Map;\nuse std::dynamic::Vec;\nfn f() -> txt { let xs: Vec<txt> = Vec.new().push(\"a\"); xs = xs.insert(9, \"b\"); return xs.join(\"\"); };\nfn main() {};",
    );
    assert!(runtime.call_root("f", Vec::new()).is_err());
    assert!(runtime.state.panicked(), "the run remembers the failure");
}

#[test]
fn an_out_of_range_remove_fails_the_run() {
    let runtime = runtime(
        "use std;\nuse std::text;\nuse std::dynamic;\nuse std::process;\nuse std::Option;\nuse std::dynamic::Map;\nuse std::dynamic::Vec;\nfn f() -> txt { let xs: Vec<txt> = Vec.new().push(\"a\"); xs = xs.remove(9); return xs.join(\"\"); };\nfn main() {};",
    );
    assert!(runtime.call_root("f", Vec::new()).is_err());
    assert!(runtime.state.panicked(), "the run remembers the failure");
}

#[test]
fn an_out_of_range_slice_fails_the_run() {
    let runtime = runtime(
        "use std;\nuse std::text;\nuse std::dynamic;\nuse std::process;\nuse std::Option;\nuse std::dynamic::Map;\nuse std::dynamic::Vec;\nfn f() -> txt { let xs: Vec<txt> = Vec.new().push(\"a\"); let ys: Vec<txt> = xs.slice(0, 9); return ys.join(\"\"); };\nfn main() {};",
    );
    assert!(runtime.call_root("f", Vec::new()).is_err());
    assert!(runtime.state.panicked(), "the run remembers the failure");
}

#[test]
fn negative_integer_patterns_evaluate() {
    expect_text_results(&[(
        "a negative integer matches",
        "use std;\nuse std::text;\nuse std::dynamic;\nuse std::process;\nuse std::Option;\nuse std::dynamic::Map;\nuse std::dynamic::Vec;\nfn f() -> txt { let n: int = -3; let label: txt = match n { -3 => \"yes\"; _ => \"no\"; }; return label; };\nfn main() {};",
        "f",
        "yes",
    )]);
}

#[test]
fn a_record_literal_is_a_match_subject() {
    expect_text_results(&[(
        "a record literal subject",
        "use std;\nuse std::text;\nuse std::dynamic;\nuse std::process;\nuse std::Option;\nuse std::dynamic::Map;\nuse std::dynamic::Vec;\nrec M { name: txt };\nfn f() -> txt { let m: M = { name = \"a\" }; let label: txt = match m { _ => \"ok\"; }; return label; };\nfn main() {};",
        "f",
        "ok",
    )]);
}

#[test]
fn recursive_types_evaluate() {
    expect_text_results(&[(
        "a self-referential tree",
        "use std;\nuse std::text;\nuse std::dynamic;\nuse std::process;\nuse std::Option;\nuse std::dynamic::Map;\nuse std::dynamic::Vec;\nrec Node { name: txt, children: Vec<Node> };\nfn f() -> txt { let leaf: Node = { name = \"leaf\", children = Vec.new() }; let root: Node = { name = \"root\", children = Vec.new().push(leaf) }; return root.children.at(0).name; };\nfn main() {};",
        "f",
        "leaf",
    )]);
}

#[test]
fn grouping_evaluates() {
    expect_text_results(&[(
        "parentheses steer precedence",
        "use std;\nuse std::text;\nuse std::dynamic;\nuse std::process;\nuse std::Option;\nuse std::dynamic::Map;\nuse std::dynamic::Vec;\nfn f() -> txt { let n: int = (1 + 2) * 3; let m: int = 10 - (2 + 3); return \"@(n):@(m)\"; };\nfn main() {};",
        "f",
        "9:5",
    )]);
}

#[test]
fn methods_evaluate() {
    expect_text_results(&[
        (
            "a Vec method counts elements",
            "use std;\nuse std::text;\nuse std::dynamic;\nuse std::process;\nuse std::Option;\nuse std::dynamic::Map;\nuse std::dynamic::Vec;\nimpl Vec { fn tally<T>(self: Vec<T>) -> int { return self.len(); }; };\nfn f() -> txt { let xs: Vec<int> = Vec.new().push(1).push(2).push(3); return \"@(xs.tally())\"; };\nfn main() {};",
            "f",
            "3",
        ),
        (
            "a declared-type method reads fields",
            "use std;\nuse std::text;\nuse std::dynamic;\nuse std::process;\nuse std::Option;\nuse std::dynamic::Map;\nuse std::dynamic::Vec;\nrec Point { x: int, y: int };\nimpl Point { fn sum(self: Point) -> int { return self.x + self.y; }; };\nfn f() -> txt { let p: Point = { x = 4, y = 5 }; return \"@(p.sum())\"; };\nfn main() {};",
            "f",
            "9",
        ),
        (
            "a method chains into another",
            "use std;\nuse std::text;\nuse std::dynamic;\nuse std::process;\nuse std::Option;\nuse std::dynamic::Map;\nuse std::dynamic::Vec;\nimpl Vec { fn tally<T>(self: Vec<T>) -> int { return self.len(); }; fn doubled<T>(self: Vec<T>) -> int { return self.tally() + self.tally(); }; };\nfn f() -> txt { let xs: Vec<int> = Vec.new().push(1).push(2); return \"@(xs.doubled())\"; };\nfn main() {};",
            "f",
            "4",
        ),
    ]);
}

#[test]
fn closures_evaluate() {
    expect_text_results(&[
        (
            "an inline closure doubles",
            "use std;\nuse std::text;\nuse std::dynamic;\nuse std::process;\nuse std::Option;\nuse std::dynamic::Map;\nuse std::dynamic::Vec;\nfn f() -> txt { let xs: Vec<int> = Vec.new().push(1).push(2).push(3); let ys: Vec<int> = xs.map(fn(x: int) -> int { return x * 2; }); return \"@(ys.at(0))@(ys.at(1))@(ys.at(2))\"; };\nfn main() {};",
            "f",
            "246",
        ),
        (
            "a closure captures an enclosing local",
            "use std;\nuse std::text;\nuse std::dynamic;\nuse std::process;\nuse std::Option;\nuse std::dynamic::Map;\nuse std::dynamic::Vec;\nfn f() -> txt { let factor: int = 10; let xs: Vec<int> = Vec.new().push(1).push(2); let ys: Vec<int> = xs.map(fn(x: int) -> int { return x * factor; }); return \"@(ys.at(0)),@(ys.at(1))\"; };\nfn main() {};",
            "f",
            "10,20",
        ),
        (
            "a nested closure captures across a boundary",
            "use std;\nuse std::text;\nuse std::dynamic;\nuse std::process;\nuse std::Option;\nuse std::dynamic::Map;\nuse std::dynamic::Vec;\nfn f() -> txt { let base: int = 100; let make: fn(int) -> fn(int) -> int = fn(a: int) -> fn(int) -> int { return fn(b: int) -> int { return base + a + b; }; }; let g: fn(int) -> int = make(20); return \"@(g(3))\"; };\nfn main() {};",
            "f",
            "123",
        ),
        (
            "a captured value is a snapshot",
            "use std;\nuse std::text;\nuse std::dynamic;\nuse std::process;\nuse std::Option;\nuse std::dynamic::Map;\nuse std::dynamic::Vec;\nfn f() -> txt { let n: int = 1; let read: fn() -> int = fn() -> int { return n; }; n = 5; return \"@(read())\"; };\nfn main() {};",
            "f",
            "1",
        ),
        (
            "a closure captures a loop variable by value",
            "use std;\nuse std::text;\nuse std::dynamic;\nuse std::process;\nuse std::Option;\nuse std::dynamic::Map;\nuse std::dynamic::Vec;\nfn f() -> txt { let xs: Vec<int> = Vec.new().push(10).push(20); let fs: Vec<fn() -> int> = Vec.new(); let index: int = 0; let count: int = xs.len(); for { match index < count { false => { break; }; true => {}; }; let x: int = xs.at(index); fs = fs.push(fn() -> int { return x; }); index = index + 1; }; let a: fn() -> int = fs.at(0); let b: fn() -> int = fs.at(1); return \"@(a()),@(b())\"; };\nfn main() {};",
            "f",
            "10,20",
        ),
        (
            "a no-value closure call is a statement",
            "use std;\nuse std::text;\nuse std::dynamic;\nuse std::process;\nuse std::Option;\nuse std::dynamic::Map;\nuse std::dynamic::Vec;\nfn f() -> txt { let set: fn() = fn() { }; set(); return \"ok\"; };\nfn main() {};",
            "f",
            "ok",
        ),
        (
            "an async closure runs and joins",
            "use std;\nuse std::text;\nuse std::dynamic;\nuse std::process;\nuse std::Option;\nuse std::dynamic::Map;\nuse std::dynamic::Vec;\nfn f() -> txt { let gate: int = 1; let work: fn() -> int = fn() -> int { return gate; }; async work(); wait; return \"ok\"; };\nfn main() {};",
            "f",
            "ok",
        ),
    ]);
}

#[test]
fn an_out_of_range_at_fails_the_run() {
    let runtime = runtime(
        "use std;\nuse std::text;\nuse std::dynamic;\nuse std::process;\nuse std::Option;\nuse std::dynamic::Map;\nuse std::dynamic::Vec;\nfn f() -> txt { let xs: Vec<txt> = Vec.new().push(\"a\"); return xs.at(9); };\nfn main() {};",
    );
    assert!(runtime.call_root("f", Vec::new()).is_err());
    assert!(runtime.state.panicked(), "the run remembers the failure");
}

#[test]
fn a_negative_at_fails_the_run() {
    let runtime = runtime(
        "use std;\nuse std::text;\nuse std::dynamic;\nuse std::process;\nuse std::Option;\nuse std::dynamic::Map;\nuse std::dynamic::Vec;\nfn f() -> txt { let xs: Vec<txt> = Vec.new().push(\"a\"); return xs.at(-1); };\nfn main() {};",
    );
    assert!(runtime.call_root("f", Vec::new()).is_err());
}

#[test]
fn an_out_of_range_replace_fails_the_run() {
    let runtime = runtime(
        "use std;\nuse std::text;\nuse std::dynamic;\nuse std::process;\nuse std::Option;\nuse std::dynamic::Map;\nuse std::dynamic::Vec;\nfn f() -> txt { let xs: Vec<txt> = Vec.new().push(\"a\"); xs = xs.replace(9, \"b\"); return xs.at(0); };\nfn main() {};",
    );
    assert!(runtime.call_root("f", Vec::new()).is_err());
    assert!(runtime.state.panicked(), "the run remembers the failure");
}

#[test]
fn replace_at_the_length_fails_the_run() {
    // `replace` never appends: an index at the length is outside the vec.
    let runtime = runtime(
        "use std;\nuse std::text;\nuse std::dynamic;\nuse std::process;\nuse std::Option;\nuse std::dynamic::Map;\nuse std::dynamic::Vec;\nfn f() -> txt { let xs: Vec<txt> = Vec.new().push(\"a\"); xs = xs.replace(1, \"b\"); return xs.at(0); };\nfn main() {};",
    );
    assert!(runtime.call_root("f", Vec::new()).is_err());
}

#[test]
fn integer_arithmetic_and_comparison_evaluate() {
    expect_text_results(&[
        (
            "addition",
            "use std;\nuse std::text;\nuse std::dynamic;\nuse std::process;\nuse std::Option;\nuse std::dynamic::Map;\nuse std::dynamic::Vec;\nfn f() -> txt { return \"@(1 + 2)\"; };\nfn main() {};",
            "f",
            "3",
        ),
        (
            "precedence multiplies before adding",
            "use std;\nuse std::text;\nuse std::dynamic;\nuse std::process;\nuse std::Option;\nuse std::dynamic::Map;\nuse std::dynamic::Vec;\nfn f() -> txt { return \"@(1 + 2 * 3)\"; };\nfn main() {};",
            "f",
            "7",
        ),
        (
            "subtraction groups left",
            "use std;\nuse std::text;\nuse std::dynamic;\nuse std::process;\nuse std::Option;\nuse std::dynamic::Map;\nuse std::dynamic::Vec;\nfn f() -> txt { return \"@(10 - 2 - 3)\"; };\nfn main() {};",
            "f",
            "5",
        ),
        (
            "division truncates",
            "use std;\nuse std::text;\nuse std::dynamic;\nuse std::process;\nuse std::Option;\nuse std::dynamic::Map;\nuse std::dynamic::Vec;\nfn f() -> txt { return \"@(7 / 2)\"; };\nfn main() {};",
            "f",
            "3",
        ),
        (
            "remainder",
            "use std;\nuse std::text;\nuse std::dynamic;\nuse std::process;\nuse std::Option;\nuse std::dynamic::Map;\nuse std::dynamic::Vec;\nfn f() -> txt { return \"@(7 % 3)\"; };\nfn main() {};",
            "f",
            "1",
        ),
        (
            "unary negation",
            "use std;\nuse std::text;\nuse std::dynamic;\nuse std::process;\nuse std::Option;\nuse std::dynamic::Map;\nuse std::dynamic::Vec;\nfn f() -> txt { return \"@(-2 - 3)\"; };\nfn main() {};",
            "f",
            "-5",
        ),
    ]);
    expect_bool_results(&[
        (
            "less than",
            "use std;\nuse std::text;\nuse std::dynamic;\nuse std::process;\nuse std::Option;\nuse std::dynamic::Map;\nuse std::dynamic::Vec;\nfn f() -> bool { return 1 < 2; };\nfn main() {};",
            "f",
            true,
        ),
        (
            "greater than",
            "use std;\nuse std::text;\nuse std::dynamic;\nuse std::process;\nuse std::Option;\nuse std::dynamic::Map;\nuse std::dynamic::Vec;\nfn f() -> bool { return 2 > 3; };\nfn main() {};",
            "f",
            false,
        ),
        (
            "less or equal",
            "use std;\nuse std::text;\nuse std::dynamic;\nuse std::process;\nuse std::Option;\nuse std::dynamic::Map;\nuse std::dynamic::Vec;\nfn f() -> bool { return 2 <= 2; };\nfn main() {};",
            "f",
            true,
        ),
        (
            "equality on integers",
            "use std;\nuse std::text;\nuse std::dynamic;\nuse std::process;\nuse std::Option;\nuse std::dynamic::Map;\nuse std::dynamic::Vec;\nfn f() -> bool { return 2 == 2; };\nfn main() {};",
            "f",
            true,
        ),
        (
            "inequality on text",
            "use std;\nuse std::text;\nuse std::dynamic;\nuse std::process;\nuse std::Option;\nuse std::dynamic::Map;\nuse std::dynamic::Vec;\nfn f() -> bool { return \"a\" != \"b\"; };\nfn main() {};",
            "f",
            true,
        ),
    ]);
}

#[test]
fn boolean_logic_evaluates() {
    expect_bool_results(&[
        (
            "not",
            "use std;\nuse std::text;\nuse std::dynamic;\nuse std::process;\nuse std::Option;\nuse std::dynamic::Map;\nuse std::dynamic::Vec;\nfn f() -> bool { return !false; };\nfn main() {};",
            "f",
            true,
        ),
        (
            "and",
            "use std;\nuse std::text;\nuse std::dynamic;\nuse std::process;\nuse std::Option;\nuse std::dynamic::Map;\nuse std::dynamic::Vec;\nfn f() -> bool { return true && false; };\nfn main() {};",
            "f",
            false,
        ),
        (
            "or",
            "use std;\nuse std::text;\nuse std::dynamic;\nuse std::process;\nuse std::Option;\nuse std::dynamic::Map;\nuse std::dynamic::Vec;\nfn f() -> bool { return false || true; };\nfn main() {};",
            "f",
            true,
        ),
        (
            "and binds tighter than or",
            "use std;\nuse std::text;\nuse std::dynamic;\nuse std::process;\nuse std::Option;\nuse std::dynamic::Map;\nuse std::dynamic::Vec;\nfn f() -> bool { return true || false && false; };\nfn main() {};",
            "f",
            true,
        ),
        (
            "and short-circuits a false left",
            "use std;\nuse std::text;\nuse std::dynamic;\nuse std::process;\nuse std::Option;\nuse std::dynamic::Map;\nuse std::dynamic::Vec;\nfn f() -> bool { return false && 1 / 0 == 0; };\nfn main() {};",
            "f",
            false,
        ),
        (
            "or short-circuits a true left",
            "use std;\nuse std::text;\nuse std::dynamic;\nuse std::process;\nuse std::Option;\nuse std::dynamic::Map;\nuse std::dynamic::Vec;\nfn f() -> bool { return true || 1 / 0 == 0; };\nfn main() {};",
            "f",
            true,
        ),
    ]);
}

#[test]
fn match_on_booleans_and_integers() {
    let runtime = runtime(
        "use std;\nuse std::text;\nuse std::dynamic;\nuse std::process;\nuse std::Option;\nuse std::dynamic::Map;\nuse std::dynamic::Vec;\nfn describe(flag: bool) -> txt { let label: txt = match flag { true => \"yes\"; false => \"no\"; }; return label; };\n\
         fn label(code: int) -> txt { let description: txt = match code { 0 => \"ok\"; 1 => \"warn\"; _ => \"other\"; }; return description; };\n\
         fn main() {};",
    );
    // A boolean match covers both values, so it needs no `_` arm.
    assert_eq!(
        runtime
            .call_root("describe", vec![Value::Bool(true)])
            .expect("describe succeeds"),
        Value::Text("yes".into())
    );
    assert_eq!(
        runtime
            .call_root("describe", vec![Value::Bool(false)])
            .expect("describe succeeds"),
        Value::Text("no".into())
    );
    assert_eq!(
        runtime
            .call_root("label", vec![Value::Int(1)])
            .expect("label succeeds"),
        Value::Text("warn".into())
    );
    assert_eq!(
        runtime
            .call_root("label", vec![Value::Int(9)])
            .expect("label succeeds"),
        Value::Text("other".into())
    );
}

#[test]
fn a_no_value_call_produces_nothing() {
    let runtime = runtime(
        "use std;\nuse std::text;\nuse std::dynamic;\nuse std::process;\nuse std::Option;\nuse std::dynamic::Map;\nuse std::dynamic::Vec;\nfn work() {};\nfn main() {};",
    );
    let value = runtime
        .call_root("work", Vec::new())
        .expect("the call succeeds");
    assert_eq!(value, Value::Nothing);
}

#[test]
fn match_takes_the_first_match_then_default() {
    let runtime = runtime(
        "use std;\nuse std::text;\nuse std::dynamic;\nuse std::process;\nuse std::Option;\nuse std::dynamic::Map;\nuse std::dynamic::Vec;\nlet a: txt = \"a\";\n\
         fn f(s: txt) -> txt {\n\
           let found: txt = \"\";\n\
           match s {\n\
             \"a\" => { found = \"one\"; };\n\
             a => { found = \"two\"; };\n\
             _ => { found = \"other\"; };\n\
           };\n\
           return found;\n\
         };\n\
         fn main() {};",
    );
    let first = runtime
        .call_root("f", vec![Value::Text("a".into())])
        .expect("the call succeeds");
    assert_eq!(first, Value::Text("one".into()));
    let fallback = runtime
        .call_root("f", vec![Value::Text("z".into())])
        .expect("the call succeeds");
    assert_eq!(fallback, Value::Text("other".into()));
}

#[test]
fn panic_stops_the_run() {
    let runtime = runtime(
        "use std;\nuse std::text;\nuse std::dynamic;\nuse std::process;\nuse std::Option;\nuse std::dynamic::Map;\nuse std::dynamic::Vec;\nfn f() { panic; };\nfn main() {};",
    );
    assert!(runtime.call_root("f", Vec::new()).is_err());
    assert!(runtime.state.panicked(), "the run remembers the panic");
}
