//! Engine tests. They call functions directly instead of running the entry,
//! so no command output is captured. Only the tests that build commands
//! spawn processes.

use std::time::{Duration, Instant};

use crate::compiler::{checked_program, prune_unreachable};
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
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn answer() -> txt { return \"42\"; };\nfn main() {};",
            "answer",
            "42",
        ),
        (
            "a present field reads as the value, in order",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nrec R { a: txt, b: txt };\nfn f() -> txt { let r: R = { a = \"1\", b = \"2\" }; let value: txt = r.b; return value; };\nfn main() {};",
            "f",
            "2",
        ),
        (
            "module values are evaluated once at startup",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nlet base: txt = \"a\";\nlet derived: txt = \"@(base)b\";\nfn f() -> txt { return \"@(derived)@(base)\"; };\nfn main() {};",
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
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn f() -> txt { let name: txt = \"world\"; return \"hello @(name)!\"; };\nfn main() {};",
            "f",
            "hello world!",
        ),
        (
            "an expression with a call",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn f() -> txt { let s: txt = \"abc\"; return \"len=@(std::text::len(s))\"; };\nfn main() {};",
            "f",
            "len=3",
        ),
        (
            "an escaped at sign stays literal",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn f() -> txt { return \"user\\@host\"; };\nfn main() {};",
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
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn f() -> txt { let s: txt = \"a\"; let label: txt = match s { \"a\" => \"one\"; _ => \"other\"; }; return label; };\nfn main() {};",
            "f",
            "one",
        ),
        (
            "the default arm's value",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn f() -> txt { let s: txt = \"z\"; let label: txt = match s { \"a\" => \"one\"; _ => \"other\"; }; return label; };\nfn main() {};",
            "f",
            "other",
        ),
        (
            "a record-valued match",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nrec R { k: txt };\nfn f() -> txt { let s: txt = \"a\"; let r: R = match s { \"a\" => { k = \"v\" }; _ => { k = \"w\" }; }; return r.k; };\nfn main() {};",
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
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nrec S { a: txt };\nfn f() -> txt { let s: S = { a = \"1\" }; let label: txt = match s { { a = \"1\" } => \"one\"; _ => \"other\"; }; return label; };\nfn main() {};",
            "f",
            "one",
        ),
        (
            "record fields match in any order",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nrec S { a: txt, b: txt };\nfn f() -> txt { let s: S = { a = \"1\", b = \"2\" }; let label: txt = match s { { b = \"2\", a = \"1\" } => \"hit\"; _ => \"miss\"; }; return label; };\nfn main() {};",
            "f",
            "hit",
        ),
        (
            "a Vec subject matches element-wise",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn f() -> txt { let s: Vec<txt> = [\"a\", \"b\"]; let label: txt = match s { [\"a\", \"b\"] => \"hit\"; _ => \"miss\"; }; return label; };\nfn main() {};",
            "f",
            "hit",
        ),
        (
            "no record match falls to the default",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nrec S { a: txt };\nfn f() -> txt { let s: S = { a = \"1\" }; let label: txt = match s { { a = \"2\" } => \"one\"; _ => \"other\"; }; return label; };\nfn main() {};",
            "f",
            "other",
        ),
        (
            "a record reference is a pattern",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nrec S { a: txt };\nfn f() -> txt { let s: S = { a = \"1\" }; let p: S = { a = \"1\" }; let label: txt = match s { p => \"hit\"; _ => \"miss\"; }; return label; };\nfn main() {};",
            "f",
            "hit",
        ),
        (
            "a Vec reference is a pattern",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn f() -> txt { let s: Vec<txt> = [\"a\"]; let p: Vec<txt> = [\"a\"]; let label: txt = match s { p => \"hit\"; _ => \"miss\"; }; return label; };\nfn main() {};",
            "f",
            "hit",
        ),
    ]);
}

#[test]
fn nominal_types_evaluate() {
    let runtime = runtime(
        "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nrec Point { name: txt, x: int };\n\
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
            "a Vec literal joins",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn f() -> txt { return std::join([\"a\", \"b\"], \"-\"); };\nfn main() {};",
            "f",
            "a-b",
        ),
        (
            "split then join",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn f() -> txt { return std::join(std::split(\"a,b,c\", \",\"), \"-\"); };\nfn main() {};",
            "f",
            "a-b-c",
        ),
        (
            "lines drops a trailing newline",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn f() -> txt { let parts: Vec<txt> = \"a\\nb\\n\".lines(); let joined: txt = std::join(parts, \"|\"); return \"@(std::text::len(joined))\"; };\nfn main() {};",
            "f",
            "3",
        ),
        (
            "for accumulates over a Vec",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn f() -> txt { let out: txt = \"\"; for x in [\"a\", \"b\", \"c\"] { out = \"@(out)@(x)\"; }; return out; };\nfn main() {};",
            "f",
            "abc",
        ),
        (
            "break ends the loop",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn f() -> txt { let out: txt = \"\"; for x in [\"a\", \"b\", \"c\"] { out = \"@(out)@(x)\"; break; }; return out; };\nfn main() {};",
            "f",
            "a",
        ),
        (
            "a match default skips an element",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn f() -> txt { let out: txt = \"\"; for x in [\"a\", \"b\", \"c\"] { match x { \"b\" => {}; _ => { out = \"@(out)@(x)\"; }; }; }; return out; };\nfn main() {};",
            "f",
            "ac",
        ),
        (
            "an unbounded for breaks",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn f() -> txt { let out: txt = \"x\"; for { out = \"y\"; break; }; return out; };\nfn main() {};",
            "f",
            "y",
        ),
        (
            "an empty Vec runs the body zero times",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn f() -> txt { let out: txt = \"x\"; for x in [] { out = \"y\"; }; return out; };\nfn main() {};",
            "f",
            "x",
        ),
        (
            "a nested loop over a split",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn f() -> txt { let last: txt = \"\"; for row in std::split(\"1;2;3\", \";\") { for cell in std::split(row, \";\") { last = cell; }; }; return last; };\nfn main() {};",
            "f",
            "3",
        ),
        (
            "a module Vec value is read",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nlet xs: Vec<txt> = [\"a\", \"b\"];\nfn f() -> txt { return std::join(xs, \"\"); };\nfn main() {};",
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
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn f() -> txt { let xs: Vec<int> = [1, 2, 3]; let s: int = 0; for x in xs { s = s + x; }; return \"@(s)\"; };\nfn main() {};",
            "f",
            "6",
        ),
        (
            "a Vec of declared recs keeps field access in a loop",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nrec Task { name: txt, weight: int };\nfn f() -> txt { let tasks: Vec<Task> = [{ name = \"a\", weight = 2 }, { name = \"b\", weight = 3 }]; let s: int = 0; for t in tasks { s = s + t.weight; }; return \"@(s)\"; };\nfn main() {};",
            "f",
            "5",
        ),
        (
            "Vec values compare by element",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn f() -> txt { let xs: Vec<int> = [1, 2]; let ys: Vec<int> = [1, 2]; let verdict: txt = match xs { ys => \"same\"; _ => \"diff\"; }; return verdict; };\nfn main() {};",
            "f",
            "same",
        ),
    ]);
}

#[test]
fn indexing_evaluates() {
    expect_text_results(&[
        (
            "an element by index",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn f() -> txt { let xs: Vec<int> = [10, 20, 30]; return \"@(xs[1])\"; };\nfn main() {};",
            "f",
            "20",
        ),
        (
            "field access through an index",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nrec T { n: txt };\nfn f() -> txt { let xs: Vec<T> = [{ n = \"a\" }, { n = \"b\" }]; return xs[1].n; };\nfn main() {};",
            "f",
            "b",
        ),
        (
            "a variable index in a loop",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn f() -> txt { let xs: Vec<txt> = [\"a\", \"b\", \"c\"]; let out: txt = \"\"; let i: int = 0; for { out = \"@(out)@(xs[i])\"; i = i + 1; match i { 3 => { break; }; _ => {}; }; }; return out; };\nfn main() {};",
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
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nenum Shape { Dot, Circle(int) };\nfn f() -> txt { let s: Shape = Shape.Circle(4); let label: txt = match s { Shape.Circle(r) => \"@(r * r)\"; Shape.Dot => \"0\"; }; return label; };\nfn main() {};",
            "f",
            "16",
        ),
        (
            "a payloadless variant matches by tag",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nenum Color { Red, Green, Blue };\nfn f() -> txt { let c: Color = Color.Green; let label: txt = match c { Color.Red => \"r\"; Color.Green => \"g\"; Color.Blue => \"b\"; }; return label; };\nfn main() {};",
            "f",
            "g",
        ),
        (
            "a variant pattern ignores a payload with underscore",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nenum Shape { Circle(int), Rect(int, int) };\nfn f() -> txt { let s: Shape = Shape.Rect(2, 3); let label: txt = match s { Shape.Circle(_) => \"circle\"; Shape.Rect(w, _) => \"@(w)\"; }; return label; };\nfn main() {};",
            "f",
            "2",
        ),
        (
            "the default arm takes the rest",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nenum Color { Red, Green, Blue };\nfn f() -> txt { let c: Color = Color.Blue; let label: txt = match c { Color.Red => \"r\"; _ => \"other\"; }; return label; };\nfn main() {};",
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
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nrec Pair<A, B> { first: A, second: B };\nfn f() -> txt { let p: Pair<int, txt> = { first = 7, second = \"z\" }; return \"@(p.first)@(p.second)\"; };\nfn main() {};",
            "f",
            "7z",
        ),
        (
            "a generic enum binds a substituted payload",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nenum Option<T> { some(T), none };\nfn f() -> txt { let o: Option<int> = Option.some(9); let label: txt = match o { Option.some(x) => \"@(x)\"; Option.none => \"none\"; }; return label; };\nfn main() {};",
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
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn id<T>(x: T) -> T { return x; };\nfn f() -> txt { return \"@(id(5))\"; };\nfn main() {};",
            "f",
            "5",
        ),
        (
            "a generic length over an inferred Vec",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn length<T>(xs: Vec<T>) -> int { let n: int = 0; for item in xs { n = n + 1; }; return n; };\nfn f() -> txt { return \"@(length([1, 2, 3]))\"; };\nfn main() {};",
            "f",
            "3",
        ),
        (
            "inference flows through a generic call",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn id<T>(x: T) -> T { return x; };\nfn wrap<T>(x: T) -> Vec<T> { return [x]; };\nfn f() -> txt { let xs: Vec<txt> = wrap(id(\"hi\")); return xs[0]; };\nfn main() {};",
            "f",
            "hi",
        ),
    ]);
}

#[test]
fn list_index_assignment_evaluates() {
    expect_text_results(&[
        (
            "index assignment replaces an element",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn f() -> txt { let xs: Vec<txt> = [\"a\", \"b\"]; xs[1] = \"c\"; return std::join(xs, \"\"); };\nfn main() {};",
            "f",
            "ac",
        ),
        (
            "assigning at the length appends",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn f() -> txt { let xs: Vec<int> = []; xs[0] = 1; xs[1] = 2; return \"@(xs[0])@(xs[1])\"; };\nfn main() {};",
            "f",
            "12",
        ),
        (
            "a loop builds a Vec in place",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn f() -> txt { let xs: Vec<int> = []; let i: int = 0; for { xs[i] = i * i; i = i + 1; match i { 4 => { break; }; _ => {}; }; }; return \"@(xs[3])\"; };\nfn main() {};",
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
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn double(x: int) -> int { return x * 2; };\nfn apply(f: fn(int) -> int, x: int) -> int { return f(x); };\nfn f() -> txt { return \"@(apply(double, 21))\"; };\nfn main() {};",
            "f",
            "42",
        ),
        (
            "map applies a function to every element",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn double(x: int) -> int { return x * 2; };\nfn f() -> txt { let xs: Vec<int> = [1, 2, 3]; let ys: Vec<int> = xs.map(double); return \"@(ys[0]),@(ys[1]),@(ys[2])\"; };\nfn main() {};",
            "f",
            "2,4,6",
        ),
        (
            "filter keeps the elements a predicate accepts",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn even(x: int) -> bool { return x % 2 == 0; };\nfn f() -> txt { let xs: Vec<int> = [1, 2, 3, 4]; let ys: Vec<int> = xs.filter(even); return \"@(ys.len())@(ys[0])\"; };\nfn main() {};",
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
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn f() -> txt { let b: bool = \"a\" < \"b\"; let label: txt = match b { true => \"yes\"; _ => \"no\"; }; return label; };\nfn main() {};",
            "f",
            "yes",
        ),
        (
            "sort orders text ascending",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn f() -> txt { let xs: Vec<txt> = [\"c\", \"a\", \"b\"]; let ys: Vec<txt> = std::vec::sorted(xs); return \"@(ys[0])@(ys[1])@(ys[2])\"; };\nfn main() {};",
            "f",
            "abc",
        ),
        (
            "sort_by orders with a comparator",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn greater(a: int, b: int) -> bool { return a > b; };\nfn f() -> txt { let xs: Vec<int> = [1, 3, 2]; let ys: Vec<int> = xs.sorted_by(greater); return \"@(ys[0])@(ys[1])@(ys[2])\"; };\nfn main() {};",
            "f",
            "321",
        ),
    ]);
}

#[test]
fn negative_integer_patterns_evaluate() {
    expect_text_results(&[(
        "a negative integer matches",
        "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn f() -> txt { let n: int = -3; let label: txt = match n { -3 => \"yes\"; _ => \"no\"; }; return label; };\nfn main() {};",
        "f",
        "yes",
    )]);
}

#[test]
fn a_record_literal_is_a_match_subject() {
    expect_text_results(&[(
        "a record literal subject",
        "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nrec M { name: txt };\nfn f() -> txt { let m: M = { name = \"a\" }; let label: txt = match m { _ => \"ok\"; }; return label; };\nfn main() {};",
        "f",
        "ok",
    )]);
}

#[test]
fn recursive_types_evaluate() {
    expect_text_results(&[(
        "a self-referential tree",
        "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nrec Node { name: txt, children: Vec<Node> };\nfn f() -> txt { let leaf: Node = { name = \"leaf\", children = [] }; let root: Node = { name = \"root\", children = [leaf] }; return root.children[0].name; };\nfn main() {};",
        "f",
        "leaf",
    )]);
}

#[test]
fn grouping_evaluates() {
    expect_text_results(&[(
        "parentheses steer precedence",
        "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn f() -> txt { let n: int = (1 + 2) * 3; let m: int = 10 - (2 + 3); return \"@(n):@(m)\"; };\nfn main() {};",
        "f",
        "9:5",
    )]);
}

#[test]
fn methods_evaluate() {
    expect_text_results(&[
        (
            "a Vec method counts elements",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn count<T>(self: Vec<T>) -> int { let n: int = 0; for item in self { n = n + 1; }; return n; };\nfn f() -> txt { let xs: Vec<int> = [1, 2, 3]; return \"@(xs.count())\"; };\nfn main() {};",
            "f",
            "3",
        ),
        (
            "a declared-type method reads fields",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nrec Point { x: int, y: int };\nfn sum(self: Point) -> int { return self.x + self.y; };\nfn f() -> txt { let p: Point = { x = 4, y = 5 }; return \"@(p.sum())\"; };\nfn main() {};",
            "f",
            "9",
        ),
        (
            "a method chains into another",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn count<T>(self: Vec<T>) -> int { let n: int = 0; for item in self { n = n + 1; }; return n; }; fn doubled<T>(self: Vec<T>) -> int { return self.count() + self.count(); };\nfn f() -> txt { let xs: Vec<int> = [1, 2]; return \"@(xs.doubled())\"; };\nfn main() {};",
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
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn f() -> txt { let ys: Vec<int> = [1, 2, 3].map(fn(x: int) -> int { return x * 2; }); return \"@(ys[0])@(ys[1])@(ys[2])\"; };\nfn main() {};",
            "f",
            "246",
        ),
        (
            "a closure captures an enclosing local",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn f() -> txt { let factor: int = 10; let ys: Vec<int> = [1, 2].map(fn(x: int) -> int { return x * factor; }); return \"@(ys[0]),@(ys[1])\"; };\nfn main() {};",
            "f",
            "10,20",
        ),
        (
            "a nested closure captures across a boundary",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn f() -> txt { let base: int = 100; let make: fn(int) -> fn(int) -> int = fn(a: int) -> fn(int) -> int { return fn(b: int) -> int { return base + a + b; }; }; let g: fn(int) -> int = make(20); return \"@(g(3))\"; };\nfn main() {};",
            "f",
            "123",
        ),
        (
            "a captured value is a snapshot",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn f() -> txt { let n: int = 1; let read: fn() -> int = fn() -> int { return n; }; n = 5; return \"@(read())\"; };\nfn main() {};",
            "f",
            "1",
        ),
        (
            "a closure captures a loop variable by value",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn f() -> txt { let fs: Vec<fn() -> int> = []; for x in [10, 20] { fs[fs.len()] = fn() -> int { return x; }; }; let a: fn() -> int = fs[0]; let b: fn() -> int = fs[1]; return \"@(a()),@(b())\"; };\nfn main() {};",
            "f",
            "10,20",
        ),
        (
            "a no-value closure call is a statement",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn f() -> txt { let set: fn() = fn() { }; set(); return \"ok\"; };\nfn main() {};",
            "f",
            "ok",
        ),
        (
            "an async closure runs and joins",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn f() -> txt { let gate: int = 1; let work: fn() -> int = fn() -> int { return gate; }; async work(); wait; return \"ok\"; };\nfn main() {};",
            "f",
            "ok",
        ),
    ]);
}

#[test]
fn an_out_of_range_index_fails_the_run() {
    let runtime = runtime(
        "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn f() -> txt { let xs: Vec<txt> = [\"a\"]; return xs[9]; };\nfn main() {};",
    );
    assert!(runtime.call_root("f", Vec::new()).is_err());
    assert!(runtime.state.panicked(), "the run remembers the failure");
}

#[test]
fn a_negative_index_fails_the_run() {
    let runtime = runtime(
        "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn f() -> txt { let xs: Vec<txt> = [\"a\"]; return xs[-1]; };\nfn main() {};",
    );
    assert!(runtime.call_root("f", Vec::new()).is_err());
}

#[test]
fn integer_arithmetic_and_comparison_evaluate() {
    expect_text_results(&[
        (
            "addition",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn f() -> txt { return \"@(1 + 2)\"; };\nfn main() {};",
            "f",
            "3",
        ),
        (
            "precedence multiplies before adding",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn f() -> txt { return \"@(1 + 2 * 3)\"; };\nfn main() {};",
            "f",
            "7",
        ),
        (
            "subtraction groups left",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn f() -> txt { return \"@(10 - 2 - 3)\"; };\nfn main() {};",
            "f",
            "5",
        ),
        (
            "division truncates",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn f() -> txt { return \"@(7 / 2)\"; };\nfn main() {};",
            "f",
            "3",
        ),
        (
            "remainder",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn f() -> txt { return \"@(7 % 3)\"; };\nfn main() {};",
            "f",
            "1",
        ),
        (
            "unary negation",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn f() -> txt { return \"@(-2 - 3)\"; };\nfn main() {};",
            "f",
            "-5",
        ),
    ]);
    expect_bool_results(&[
        (
            "less than",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn f() -> bool { return 1 < 2; };\nfn main() {};",
            "f",
            true,
        ),
        (
            "greater than",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn f() -> bool { return 2 > 3; };\nfn main() {};",
            "f",
            false,
        ),
        (
            "less or equal",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn f() -> bool { return 2 <= 2; };\nfn main() {};",
            "f",
            true,
        ),
        (
            "equality on integers",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn f() -> bool { return 2 == 2; };\nfn main() {};",
            "f",
            true,
        ),
        (
            "inequality on text",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn f() -> bool { return \"a\" != \"b\"; };\nfn main() {};",
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
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn f() -> bool { return !false; };\nfn main() {};",
            "f",
            true,
        ),
        (
            "and",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn f() -> bool { return true && false; };\nfn main() {};",
            "f",
            false,
        ),
        (
            "or",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn f() -> bool { return false || true; };\nfn main() {};",
            "f",
            true,
        ),
        (
            "and binds tighter than or",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn f() -> bool { return true || false && false; };\nfn main() {};",
            "f",
            true,
        ),
        (
            "and short-circuits a false left",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn f() -> bool { return false && 1 / 0 == 0; };\nfn main() {};",
            "f",
            false,
        ),
        (
            "or short-circuits a true left",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn f() -> bool { return true || 1 / 0 == 0; };\nfn main() {};",
            "f",
            true,
        ),
    ]);
}

#[test]
fn match_on_booleans_and_integers() {
    let runtime = runtime(
        "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn describe(flag: bool) -> txt { let label: txt = match flag { true => \"yes\"; false => \"no\"; }; return label; };\n\
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
        "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn work() {};\nfn main() {};",
    );
    let value = runtime
        .call_root("work", Vec::new())
        .expect("the call succeeds");
    assert_eq!(value, Value::Nothing);
}

#[test]
fn match_takes_the_first_match_then_default() {
    let runtime = runtime(
        "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nlet a: txt = \"a\";\n\
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
        "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn f() { panic; };\nfn main() {};",
    );
    assert!(runtime.call_root("f", Vec::new()).is_err());
    assert!(runtime.state.panicked(), "the run remembers the panic");
}

#[test]
fn async_does_not_wait_for_the_call() {
    let runtime = runtime(
        "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn worker() -> txt { return \"w\"; };\n\
         fn f() -> txt { async worker(); return \"done\"; };\n\
         fn main() {};",
    );
    assert_eq!(call_text(&runtime, "f"), "done");
}

#[test]
fn wait_blocks_until_the_thread_finishes() {
    let directory = tempfile::tempdir().expect("temp dir");
    let marker = directory.path().join("finished");
    let source = format!(
        "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn worker() {{ std::command(\"sleep 0.2; touch {}\").run(); }};\n\
         fn f() -> txt {{ async worker(); wait; return \"done\"; }};\n\
         fn main() {{}};",
        marker.display()
    );
    assert_eq!(call_text(&runtime(&source), "f"), "done");
    assert!(marker.exists(), "wait joined the thread before continuing");
}

#[test]
fn wait_joins_every_outstanding_thread() {
    let directory = tempfile::tempdir().expect("temp dir");
    let first = directory.path().join("first");
    let second = directory.path().join("second");
    let third = directory.path().join("third");
    let source = format!(
        "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn work(marker: txt) {{ std::command(\"sleep 0.2; touch @(marker)\").run(); }};\n\
         fn f() -> txt {{\n\
           async work(\"{first}\");\n\
           async work(\"{second}\");\n\
           async work(\"{third}\");\n\
           wait;\n\
           return \"done\";\n\
         }};\nfn main() {{}};",
        first = first.display(),
        second = second.display(),
        third = third.display(),
    );
    assert_eq!(call_text(&runtime(&source), "f"), "done");
    assert!(first.exists(), "the first thread finished");
    assert!(second.exists(), "the second thread finished");
    assert!(third.exists(), "the third thread finished");
}

#[test]
fn a_second_wait_returns_immediately() {
    let runtime = runtime(
        "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn worker() {};\n\
         fn f() -> txt {\n\
           async worker();\n\
           wait;\n\
           wait;\n\
           return \"done\";\n\
         };\n\
         fn main() {};",
    );
    assert_eq!(call_text(&runtime, "f"), "done");
}

#[test]
fn wait_inside_a_spawned_thread_returns_without_joining_its_spawner() {
    let runtime = runtime(
        "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn worker() { wait; };\n\
         fn f() -> txt { async worker(); wait; return \"done\"; };\n\
         fn main() {};",
    );
    assert_eq!(call_text(&runtime, "f"), "done");
}

#[test]
fn wait_inside_a_spawned_thread_joins_its_own_children() {
    let directory = tempfile::tempdir().expect("temp dir");
    let marker = directory.path().join("finished");
    let source = format!(
        "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn child() {{ std::command(\"sleep 0.2; touch {}\").run(); }};\n\
         fn worker() {{ async child(); wait; }};\n\
         fn f() -> txt {{ async worker(); wait; return \"done\"; }};\n\
         fn main() {{}};",
        marker.display()
    );
    assert_eq!(call_text(&runtime(&source), "f"), "done");
    assert!(
        marker.exists(),
        "the inner wait joined the thread worker spawned"
    );
}

#[test]
fn panic_in_an_async_thread_sets_the_panicked_flag() {
    let runtime = runtime(
        "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn worker() { panic; };\n\
         fn f() -> txt { async worker(); return \"done\"; };\n\
         fn main() {};",
    );
    // The detached panic can reach the caller before or after `f` returns.
    let _ = runtime.call_root("f", Vec::new());
    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline && !runtime.state.panicked() {
        std::thread::sleep(Duration::from_millis(5));
    }
    assert!(
        runtime.state.panicked(),
        "a panic in a detached thread is remembered"
    );
}

#[test]
fn panic_in_a_waited_thread_fails_the_run() {
    let runtime = runtime(
        "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn worker() { panic; };\n\
         fn f() -> txt { async worker(); wait; return \"done\"; };\nfn main() {};",
    );
    let _ = runtime.call_root("f", Vec::new());
    assert!(runtime.state.panicked(), "the joined panic fails the run");
}

#[test]
fn async_over_a_command_call_runs_on_the_thread() {
    let directory = tempfile::tempdir().expect("temp dir");
    let marker = directory.path().join("ran");
    let source = format!(
        "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn f() -> txt {{\n\
           async std::command(\"touch {}\").run();\n\
           wait;\n\
           return \"done\";\n\
         }};\n\
         fn main() {{}};",
        marker.display()
    );
    assert_eq!(call_text(&runtime(&source), "f"), "done");
    assert!(marker.exists(), "the call ran on the spawned thread");
}

#[test]
fn async_of_a_command_call_runs_in_its_directory() {
    let directory = tempfile::tempdir().expect("temp dir");
    let marker = directory.path().join("ran");
    let source = format!(
        "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn f() -> txt {{\n\
           async std::command(\"touch ran\").dir(\"{}\").run();\n\
           wait;\n\
           return \"done\";\n\
         }};\n\
         fn main() {{}};",
        directory.path().display()
    );
    assert_eq!(call_text(&runtime(&source), "f"), "done");
    assert!(
        marker.exists(),
        "the command ran in the directory named by the spec"
    );
}

#[test]
fn a_panic_cancels_other_threads_commands() {
    let directory = tempfile::tempdir().expect("temp dir");
    let marker = directory.path().join("survived");
    let source = format!(
        "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn worker() {{ panic; }};\n\
         fn other() {{ std::command(\"sleep 0.2; touch {}\").run(); }};\n\
         fn f() -> txt {{\n\
           async worker();\n\
           async other();\n\
           wait;\n\
           return \"done\";\n\
         }};\n\
         fn main() {{}};",
        marker.display()
    );
    let runtime = runtime(&source);
    let _ = runtime.call_root("f", Vec::new());
    assert!(runtime.state.panicked(), "the panic is recorded");
    assert!(
        !marker.exists(),
        "a panic cancels the other thread's command"
    );
}

#[test]
fn a_retained_program_still_runs() {
    let mut program = checked_program(
        "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn answer() -> txt { return \"42\"; };\n\
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
        "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nlet base: txt = \"a\";\n\
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
