use crate::compiler::{checked_files, checked_program};
use crate::model::Type;
use crate::model::{DeclarationId, Program};

fn check(files: &[(&str, &str)], entry: &str) -> Result<(), String> {
    checked_files(files, entry).map(|_| ())
}

/// The declaration a namespace path and a name resolve to, searched through
/// both registries.
fn declaration(program: &Program, namespace: &[&str], name: &str) -> DeclarationId {
    let path: Vec<String> = namespace
        .iter()
        .map(|segment| (*segment).to_owned())
        .collect();
    let namespace = program.namespace_at(&path).expect("the namespace exists");
    program
        .namespace(namespace)
        .get(name)
        .first()
        .copied()
        .expect("the name exists")
}

/// The first parameter of a callable declaration.
fn first_parameter(program: &Program, id: DeclarationId) -> DeclarationId {
    program
        .declaration(id)
        .function()
        .expect("a callable has a function value")
        .parameters[0]
}

/// The type a node declares: a callable's return type (or `None` when it
/// declares none), or a value or binding's type.
fn value_type(program: &Program, id: DeclarationId) -> Option<Type> {
    let declaration = program.declaration(id);
    if declaration.function().is_some() || declaration.is_native() {
        return declaration.return_type();
    }
    Some(declaration.ty.clone())
}

/// Run every acceptance case, reporting all failures at once so one run names
/// every case that broke rather than only the first.
fn expect_accepts(cases: &[(&str, &str)]) {
    let mut failures = Vec::new();
    for &(name, source) in cases {
        if let Err(message) = check(&[("main.kiru", source)], "main.kiru") {
            failures.push(format!("{name}: expected acceptance, found `{message}`"));
        }
    }
    assert!(
        failures.is_empty(),
        "{} acceptance case(s) failed:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

/// Run every rejection case against its expected message, collecting all
/// mismatches so one run reports every case that failed.
fn expect_rejections(cases: &[(&str, &str, &str)]) {
    let mut failures = Vec::new();
    for &(name, source, expected) in cases {
        match check(&[("main.kiru", source)], "main.kiru") {
            Ok(()) => failures.push(format!("{name}: expected `{expected}`, but it checked")),
            Err(message) if message != expected => {
                failures.push(format!("{name}: expected `{expected}`, found `{message}`"));
            }
            Err(_) => {}
        }
    }
    assert!(
        failures.is_empty(),
        "{} rejection case(s) failed:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[test]
fn entry_and_return_rules() {
    expect_accepts(&[
        (
            "main without a parameter",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn main() {};",
        ),
        (
            "main with the words parameter",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn main(words: Vec<txt>) {};",
        ),
        (
            "function without a return",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn f() { let x: txt = \"a\"; };\nfn main() {};",
        ),
        (
            "function with a return",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn f() -> txt { return \"a\"; };\nfn main() {};",
        ),
        (
            "an early return in a match arm with a final return",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn f(s: txt) -> txt { match s { \"a\" => { return \"a\"; }; }; return \"\"; };\nfn main() {};",
        ),
        (
            "unreachable code after a return",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn f() -> txt { return \"a\"; return \"b\"; };\nfn main() {};",
        ),
        (
            "a bare return in main",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn main() { return; };",
        ),
        (
            "a value-returning main",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn main() -> int { return 0; };",
        ),
        (
            "a value function that panics on every other path",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn f(s: txt) -> txt { match s { \"a\" => { return \"a\"; }; _ => { panic; }; }; };\nfn main() {};",
        ),
        (
            "a value function that panics on every path",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn f() -> txt { panic; };\nfn main() {};",
        ),
        (
            "a value function that ends in a halting call",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn f() -> txt { std::fail(\"x\"); };\nfn main() {};",
        ),
        (
            "a value function that ends in a halting user function",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn stop() { panic; };\nfn f() -> txt { stop(); };\nfn main() {};",
        ),
        (
            "halting is transitive",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn inner() { panic; };\nfn outer() { inner(); };\nfn f() -> txt { outer(); };\nfn main() {};",
        ),
    ]);
    expect_rejections(&[
        (
            "a value function that can fall through",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn f(s: txt) -> txt { match s { \"a\" => { return \"a\"; }; }; };\nfn main() {};",
            "`f` is declared to return text, so every path must end with `return ...`, `panic;`, or a call that stops the run",
        ),
        (
            "a returned value of the wrong type",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn f(s: txt) -> txt { match s { \"a\" => { return \"a\"; }; _ => { return { k = \"v\" }; }; }; };\nfn main() {};",
            "expected text, found a `{ … }` literal",
        ),
        (
            "returning a nothing call",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn work() {};\nfn f() -> txt { return work(); };\nfn main() {};",
            "expected text, found no value",
        ),
        (
            "a value return in a nothing function",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn f() { return \"a\"; };\nfn main() {};",
            "`f` is declared to return nothing, so `return` cannot carry a value",
        ),
        (
            "a bare return in a value function",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn f() -> txt { return; };\nfn main() {};",
            "`f` is declared to return text, so `return` must carry a value",
        ),
    ]);
}

#[test]
fn panic_and_no_value_rules() {
    expect_accepts(&[(
        "panic as a statement",
        "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn main() { panic; };",
    )]);
    expect_rejections(&[
        (
            "fail bound to text",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn main() { let x: txt = std::fail(\"x\"); };",
            "expected text, found no value",
        ),
        (
            "fail passed as a text argument",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn take(x: txt) {};\nfn main() { take(std::fail(\"x\")); };",
            "expected text, found no value",
        ),
        (
            "fail in a record field",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nrec R { k: txt };\nfn main() { let r: R = { k = std::fail(\"x\") }; };",
            "expected text, found no value",
        ),
    ]);
}

#[test]
fn command_and_field_rules() {
    expect_accepts(&[
        (
            "command code returned through a function",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn code(line: txt) -> int { return std::command(line).run(); };\nfn main() { let code: int = code(\"echo hi\"); };",
        ),
        (
            "command at a module value",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nlet x: int = std::command(\"echo hi\").run();\nfn main() {};",
        ),
        (
            "a call inside a record",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nrec R { code: txt };\nlet r: R = { code = std::quote(\"echo hi\") };\nfn main() {};",
        ),
        (
            "command as a bare statement",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn main() { std::command(\"true\").run(); };",
        ),
    ]);
    expect_rejections(&[
        (
            "command bound to a record",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nrec R { a: txt };\nfn main() { let x: R = std::command(\"ls\").run(); };",
            "expected R, found int",
        ),
        (
            "command passed as a record argument",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nrec R { a: txt };\nfn take(x: R) {};\nfn main() { take(std::command(\"ls\").run()); };",
            "expected R, found int",
        ),
        (
            "command without arguments",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn main() { std::command(); };",
            "`std::command` takes 1 argument, found 0",
        ),
        (
            "command with two arguments",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn main() { std::command(\"ls\", \"x\"); };",
            "`std::command` takes 1 argument, found 2",
        ),
        (
            "command with a non-text line",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn main() { std::command(1); };",
            "expected text, found int",
        ),
        (
            "field access on text",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn main() { let x: txt = \"a\".b; };",
            "expected a rec, found text",
        ),
    ]);
}

#[test]
fn text_and_record_rules() {
    expect_accepts(&[
        (
            "record argument",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nrec Repo { dir: txt };\nfn read(repo: Repo) -> txt { return repo.dir; };\nfn main() { read({ dir = \"/x\" }); };",
        ),
        (
            "record parameter through a call",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nrec Repo { dir: txt };\nlet backend: Repo = { dir = \"/b\" };\nfn read_it(repo: Repo) -> txt { return repo.dir; };\nfn main() { read_it(backend); };",
        ),
        (
            "text parameter through a call",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn identity(value: txt) -> txt { return value; };\nfn main() { std::print(identity(\"x\")); };",
        ),
        (
            "record parameter through a call with a literal",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nrec Repo { dir: txt };\nfn identity(repo: Repo) -> txt { return repo.dir; };\nfn main() { std::print(identity({ dir = \"/x\" })); };",
        ),
    ]);
    expect_rejections(&[
        (
            "arithmetic on text",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn main() { let x: int = \"a\" + \"b\"; };",
            "expected int, found text",
        ),
        (
            "text parameter used as a record",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn f(x: txt) { let a: txt = x; let b: txt = x.b; };\nfn main() { f(\"s\"); };",
            "expected a rec, found text",
        ),
        (
            "text where a record is required",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nrec Repo { dir: txt };\nfn read(repo: Repo) -> txt { return repo.dir; };\nfn main() { read(\"x\"); };",
            "expected Repo, found text",
        ),
    ]);
}

#[test]
fn async_wait_rules() {
    expect_accepts(&[
        (
            "nothing function as a statement and an async spawn",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn work() {};\nfn main() {\nwork();\nasync work();\nwait;\n};",
        ),
        (
            "async of a command call",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn main() { async std::command(\"true\").run(); };",
        ),
        (
            "async of a call returning text",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn pick() -> txt { return \"x\"; };\nfn main() { async pick(); wait; };",
        ),
        (
            "async of a halting call does not stop the caller",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn stop() { panic; };\nfn main() { async stop(); };",
        ),
        (
            "bare async statement",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn work() {};\nfn main() { async work(); };",
        ),
        (
            "wait as a statement",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn work() {};\nfn main() { async work(); wait; };",
        ),
    ]);
    expect_rejections(&[
        (
            "async of a non-call",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn main() { async \"work\"; };",
            "`async` takes a call",
        ),
        (
            "async of a text variable",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nlet x: txt = \"a\";\nfn main() { async x; };",
            "`async` takes a call",
        ),
    ]);
}

#[test]
fn bindings_and_scope_rules() {
    expect_accepts(&[
        (
            "literal, name, and field-path match arms",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nrec Spec { cmd: txt };\nlet spec: Spec = { cmd = \"a\" };\nfn main() {\nlet a: txt = \"a\";\nmatch \"a\" { \"a\" => {}; a => {}; spec.cmd => {}; };\n};",
        ),
        (
            "parameter assignment",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn f(x: txt) { x = \"a\"; };\nfn main() {};",
        ),
    ]);
    expect_rejections(&[
        (
            "module assignment",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nlet x: txt = \"a\";\nfn main() { x = \"b\"; };",
            "`x` is a module-level constant and cannot be assigned",
        ),
        (
            "assigning a record literal to a text local",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn main() { let x: txt = \"a\"; x = {}; };",
            "expected text, found a `{ … }` literal",
        ),
        (
            "duplicate literal match pattern",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn main() { match \"a\" { \"a\" => {}; \"a\" => {}; }; };",
            "duplicate match pattern `a`",
        ),
        (
            "text arm against a text subject only",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn main() { match \"a\" { {} => {}; }; };",
            "expected text, found a `{ … }` literal",
        ),
        (
            "match arm does not leak locals",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn main() {\nmatch \"a\" {\n\"a\" => { let hidden: txt = \"x\"; };\n\"b\" => { let seen: txt = hidden; };\n};\n};",
            "unknown name `hidden`",
        ),
        (
            "a call match arm",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn pick() -> txt { return \"a\"; };\nfn main() {\nmatch \"a\" { pick() => {}; };\n};",
            "a match arm is a literal, a name, or a field path",
        ),
        (
            "a concatenation match arm",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn main() {\nmatch \"ab\" { \"a\" + \"b\" => {}; };\n};",
            "a match arm is a literal, a name, or a field path",
        ),
        (
            "duplicate reference match patterns",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn main() {\nlet a: txt = \"a\";\nmatch \"a\" { a => {}; a => {}; };\n};",
            "duplicate match pattern",
        ),
    ]);
}

#[test]
fn match_expression_rules() {
    expect_accepts(&[
        (
            "an expression match yields a value",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn f() -> txt { let label: txt = match \"a\" { \"a\" => \"one\"; _ => \"other\"; }; return label; };\nfn main() {};",
        ),
        (
            "a record-valued match",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nrec R { k: txt };\nfn f() -> txt { let r: R = match \"a\" { \"a\" => { k = \"v\" }; _ => { k = \"w\" }; }; return r.k; };\nfn main() {};",
        ),
    ]);
    expect_rejections(&[
        (
            "match arms of different types",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn main() { let x: txt = match \"a\" { \"a\" => \"one\"; _ => { k = \"v\" }; }; };",
            "expected text, found a `{ … }` literal",
        ),
        (
            "a text match assigned to a record",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nrec R { k: txt };\nfn main() { let x: R = match \"a\" { \"a\" => \"one\"; _ => \"two\"; }; };",
            "expected R, found text",
        ),
    ]);
}

#[test]
fn match_on_records_and_lists() {
    expect_accepts(&[
        (
            "a record subject with a record literal arm",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nrec S { a: txt };\nfn main() {\nlet s: S = { a = \"1\" };\nmatch s { { a = \"1\" } => {}; _ => {}; };\n};",
        ),
        (
            "a Vec subject with a Vec literal arm",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn main() {\nlet s: Vec<txt> = [\"a\", \"b\"];\nmatch s { [\"a\", \"b\"] => {}; _ => {}; };\n};",
        ),
        (
            "a record-valued match yields a value",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nrec S { a: txt };\nfn f() -> txt { let s: S = { a = \"1\" }; let label: txt = match s { { a = \"1\" } => \"one\"; _ => \"other\"; }; return label; };\nfn main() {};",
        ),
        (
            "a record reference is a pattern",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nrec S { a: txt };\nfn main() {\nlet s: S = { a = \"1\" };\nlet p: S = { a = \"1\" };\nmatch s { p => {}; _ => {}; };\n};",
        ),
        (
            "a Vec reference is a pattern",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn main() {\nlet s: Vec<txt> = [\"a\"];\nlet p: Vec<txt> = [\"a\"];\nmatch s { p => {}; _ => {}; };\n};",
        ),
        (
            "a statement match may omit the default",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nrec S { a: txt };\nfn main() {\nlet s: S = { a = \"1\" };\nmatch s { { a = \"1\" } => {}; };\n};",
        ),
    ]);
    expect_rejections(&[
        (
            "a text arm against a record subject",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nrec S { a: txt };\nfn main() { let s: S = { a = \"1\" }; match s { \"a\" => {}; }; };",
            "expected S, found text",
        ),
        (
            "a record arm against a Vec subject",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn main() { let s: Vec<txt> = [\"a\"]; match s { {} => {}; }; };",
            "expected Vec<text>, found a `{ … }` literal",
        ),
        (
            "a value match without a default",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn f() -> txt { let label: txt = match \"a\" { \"a\" => \"one\"; }; return label; };\nfn main() {};",
            "a match that produces a value must cover every case; add a `_` arm",
        ),
        (
            "a bound value match without a default",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn main() { let x: txt = match \"a\" { \"a\" => \"one\"; }; };",
            "a match that produces a value must cover every case; add a `_` arm",
        ),
        (
            "duplicate record arms",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nrec S { a: txt };\nfn main() { let s: S = { a = \"1\" }; match s { { a = \"1\" } => {}; { a = \"1\" } => {}; }; };",
            "duplicate match pattern",
        ),
        (
            "duplicate Vec arms",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn main() { let s: Vec<txt> = [\"a\"]; match s { [\"a\"] => {}; [\"a\"] => {}; }; };",
            "duplicate match pattern",
        ),
        (
            "record arms equal whatever the field order",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nrec S { a: txt, b: txt };\nfn main() { let s: S = { a = \"1\", b = \"2\" }; match s { { a = \"1\", b = \"2\" } => {}; { b = \"2\", a = \"1\" } => {}; }; };",
            "duplicate match pattern",
        ),
        (
            "a call inside a record arm",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nrec S { a: txt };\nfn g() -> txt { return \"x\"; };\nfn main() { let s: S = { a = \"1\" }; match s { { a = g() } => {}; }; };",
            "a match arm is a literal, a name, or a field path",
        ),
        (
            "a no-value subject",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn g() {};\nfn main() { match g() { _ => {}; }; };",
            "a match subject must have a value",
        ),
    ]);
}

#[test]
fn rec_fields_must_be_unique() {
    expect_accepts(&[(
        "distinct fields",
        "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nrec R { a: txt, b: txt };\nfn main() { let r: R = { a = \"1\", b = \"2\" }; };",
    )]);
    expect_rejections(&[
        (
            "a local repeats a field",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nrec R { a: txt };\nfn main() { let r: R = { a = \"1\", a = \"2\" }; };",
            "duplicate field `a`",
        ),
        (
            "a literal repeats a field",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nrec R { a: txt };\nfn main() { let r: R = { a = \"1\", a = \"2\" }; };",
            "duplicate field `a`",
        ),
        (
            "an argument record repeats a field",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nrec R { a: txt };\nfn f(r: R) {};\nfn main() { f({ a = \"1\", a = \"2\" }); };",
            "duplicate field `a`",
        ),
        (
            "a module value repeats a field",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nrec R { a: txt };\nlet r: R = { a = \"1\", a = \"2\" };\nfn main() {};",
            "duplicate field `a`",
        ),
    ]);
}

#[test]
fn return_does_not_take_a_match() {
    expect_rejections(&[(
        "a match returned directly",
        "use std;\nuse std::int;\nuse std::text;\nfn f(n: int) -> txt { return match n { 1 => \"one\"; _ => \"other\"; }; };\nfn main() {};",
        "`return` does not take a `match`; bind it first: `let name = match ...; return name;`",
    )]);
}

#[test]
fn nominal_type_rules() {
    expect_accepts(&[
        (
            "a declared type used as an annotation",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nrec Project { name: txt, count: int };\nfn main() { let p: Project = { name = \"x\", count = 3 }; };",
        ),
        (
            "a field has the field's type",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nrec Project { count: int };\nfn f(p: Project) -> int { return p.count; };\nfn main() {};",
        ),
        (
            "a record field is assigned a fitting value",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nrec Project { count: int };\nfn f(p: Project) { p.count = p.count + 1; };\nfn main() {};",
        ),
        (
            "equality with a declared type reads either way",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nrec P { a: txt };\nfn f(p: P) -> bool { return { a = \"x\" } == p && p == { a = \"x\" }; };\nfn main() {};",
        ),
        (
            "a nominal literal",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nrec P { a: txt };\nfn main() { let p: P = { a = \"x\" }; };",
        ),
        (
            "a nominal literal as a return value",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nrec P { a: txt };\nfn f() -> P { return { a = \"x\" }; };\nfn main() {};",
        ),
        (
            "a self-referential type",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nrec Node { name: txt, children: Vec<Node> };\nfn main() { let leaf: Node = { name = \"a\", children = [] }; let root: Node = { name = \"r\", children = [leaf] }; };",
        ),
        (
            "mutually recursive types",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nenum Json { Null, Int(int), Array(Vec<Json>), Object(Vec<Field>) };\nrec Field { key: txt, value: Json };\nfn main() { let v: Json = Json.Array([Json.Null]); };",
        ),
    ]);
    expect_rejections(&[
        (
            "a missing field",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nrec Project { name: txt };\nfn main() { let p: Project = {}; };",
            "`Project` is missing field `name`",
        ),
        (
            "an extra field",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nrec Project { name: txt };\nfn main() { let p: Project = { name = \"x\", extra = \"y\" }; };",
            "`Project` has no field `extra`",
        ),
        (
            "a field of the wrong type",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nrec Project { count: int };\nfn main() { let p: Project = { count = \"x\" }; };",
            "expected int, found text",
        ),
        (
            "an unknown declared type",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn main() { let p: Missing = {}; };",
            "unknown type `Missing`",
        ),
        (
            "a duplicate field in a declaration",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nrec Project { name: txt, name: txt };\nfn main() {};",
            "duplicate field `name`",
        ),
        (
            "a duplicate field in a literal",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nrec Project { name: txt };\nfn main() { let p: Project = { name = \"x\", name = \"y\" }; };",
            "duplicate field `name`",
        ),
        (
            "an unknown field access",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nrec Project { name: txt };\nfn f(p: Project) -> txt { return p.missing; };\nfn main() {};",
            "`Project` has no field `missing`",
        ),
        (
            "a nominal literal missing a field",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nrec Project { name: txt };\nfn main() { let p: Project = {}; };",
            "`Project` is missing field `name`",
        ),
    ]);
}

#[test]
fn comparison_rules() {
    expect_accepts(&[
        (
            "integer ordering",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn main() { let b: bool = 1 < 2; };",
        ),
        (
            "text ordering",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn main() { let b: bool = \"a\" < \"b\"; };",
        ),
    ]);
    expect_rejections(&[
        (
            "ordering a record",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nrec S { a: txt };\nfn main() { let a: S = { a = \"1\" }; let b: S = { a = \"2\" }; let c: bool = a < b; };",
            "`S` cannot be ordered; compare `int` or `txt`",
        ),
        (
            "ordering a boolean",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn main() { let b: bool = true < false; };",
            "`bool` cannot be ordered; compare `int` or `txt`",
        ),
    ]);
}

#[test]
fn name_and_call_rules() {
    expect_accepts(&[(
        "root access when a local shadows the root",
        "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\npub let value: txt = \"root\";\nfn main() {\nlet value: txt = \"local\";\nstd::print(value);\nstd::print(::value);\n};",
    )]);
    expect_rejections(&[
        (
            "a value and a function share a name",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn build() -> txt { return \"built\"; };\nlet build: txt = \"text\";\nfn main() { std::print(build); };",
            "`build` is declared more than once in this namespace",
        ),
        (
            "function value where a text is expected",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn f() {};\nfn main() { let x: txt = f; };",
            "expected text, found fn()",
        ),
        (
            "calling a module value",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nlet x: txt = \"a\";\nfn main() { x(); };",
            "`x` is a value and cannot be called",
        ),
        (
            "calling a local",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn main() { let x: txt = \"a\"; x(); };",
            "`x` is a value and cannot be called",
        ),
        (
            "wrong arity",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn f(a: txt) {};\nfn main() { f(); };",
            "`f` takes 1 argument, found 0",
        ),
        (
            "unknown rooted name",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nlet value: txt = \"root\";\nfn main() { std::print(::missing); };",
            "unknown name `::missing`",
        ),
        (
            "function self reference",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn f() { f(); };\nfn main() {};",
            "a name cannot reference itself",
        ),
        (
            "value self reference",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nlet x: txt = x;\nfn main() {};",
            "a name cannot reference itself",
        ),
    ]);
}

#[test]
fn statement_form_rules() {
    expect_accepts(&[(
        "bare call and panic statements",
        "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn work() {};\nfn main() {\nwork();\nstd::command(\"true\").run();\npanic;\n};",
    )]);
    expect_rejections(&[
        (
            "bare literal statement",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn main() { \"x\"; };",
            "a statement must be a call",
        ),
        (
            "bare reference statement",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn main() { let name: txt = \"x\"; name; };",
            "a statement must be a call",
        ),
        (
            "bare record statement",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn main() { { k = \"v\" }; };",
            "a statement must be a call",
        ),
        (
            "bare add statement",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn main() { \"a\" + \"b\"; };",
            "a statement must be a call",
        ),
    ]);
}

#[test]
fn assignment_rules() {
    expect_accepts(&[
        (
            "rebind a text",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn f() { let x: txt = \"a\"; x = \"b\"; };\nfn main() {};",
        ),
        (
            "field assign a record",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nrec R { a: txt };\nfn f() { let r: R = { a = \"1\" }; r.a = \"2\"; };\nfn main() {};",
        ),
        (
            "rebind a parameter",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn f(x: txt) { x = \"b\"; };\nfn main() {};",
        ),
        (
            "field assign a parameter",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nrec R { a: txt };\nfn f(r: R) { r.a = \"b\"; };\nfn main() {};",
        ),
    ]);
    expect_rejections(&[
        (
            "field assign a text binding",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn f() { let x: txt = \"a\"; x.b = \"c\"; };\nfn main() {};",
            "`x` is not a rec and has no fields",
        ),
        (
            "field assign a module value",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nrec R { a: txt };\nlet r: R = { a = \"1\" };\nfn main() { r.a = \"2\"; };",
            "`r` is a module-level constant and cannot be assigned",
        ),
    ]);
}

#[test]
fn checks_the_shipped_library_types() {
    let program = checked_program(
        "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nrec Repo { dir: txt };\n\
         let backend: Repo = { dir = \"/kiru\" };\n\
         fn read(repo: Repo) -> txt { return repo.dir; };\n\
         fn main() {\n\
           std::print(\"x\");\n\
           read(backend);\n\
         };",
    );

    let command = declaration(&program, &["std"], "command");
    let Some(Type::Named(id, arguments)) = value_type(&program, command) else {
        panic!("`std::command` returns a declared type");
    };
    assert!(arguments.is_empty());
    assert_eq!(program.types[id.0].name, "Command");

    let print = declaration(&program, &["std"], "print");
    assert_eq!(
        value_type(&program, first_parameter(&program, print)),
        Some(Type::Text)
    );
    assert_eq!(value_type(&program, print), None);

    let eprint = declaration(&program, &["std"], "fail");
    assert_eq!(value_type(&program, eprint), None);
    assert!(
        program.declaration(eprint).derived.halts,
        "fail ends in `panic;`, so it stops the run"
    );

    let read = declaration(&program, &[], "read");
    let Some(Type::Named(id, _)) = value_type(&program, first_parameter(&program, read)) else {
        panic!("`read` takes a declared type");
    };
    assert_eq!(program.types[id.0].name, "Repo");
}

#[test]
fn list_and_loop_rules() {
    expect_accepts(&[
        (
            "a Vec literal binding and a for loop",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn main() { let xs: Vec<txt> = [\"a\", \"b\"]; for x in xs { std::print(x); }; };",
        ),
        (
            "a function that takes and returns a Vec",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn id(xs: Vec<txt>) -> Vec<txt> { return xs; }; fn main() { id([\"a\"]); };",
        ),
        (
            "a module-level Vec value",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nlet xs: Vec<txt> = [\"a\"]; fn main() { for x in xs { std::print(x); }; };",
        ),
        (
            "break inside a for",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn main() { for x in [\"a\"] { break; }; };",
        ),
        (
            "a for that skips an element with a match default",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn main() { for x in [\"a\"] { match x { \"a\" => {}; _ => { std::print(x); }; }; }; };",
        ),
        (
            "an unbounded for that breaks",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn main() { for { break; }; };",
        ),
        (
            "an unbounded for as the only path of a value function",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn f() -> txt { for { std::print(\"a\"); }; }; fn main() {};",
        ),
    ]);
    expect_rejections(&[
        (
            "for over text",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn main() { for x in \"a\" { std::print(x); }; };",
            "expected Vec, found text",
        ),
        (
            "break outside a loop",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn main() { break; };",
            "`break` is only allowed inside a loop",
        ),
        (
            "a Vec element that is not text",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn main() { let xs: Vec<txt> = [{ k = \"v\" }]; };",
            "expected text, found a `{ … }` literal",
        ),
        (
            "a Vec binding whose value is not a Vec",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn main() { let xs: Vec<txt> = \"a\"; };",
            "expected Vec<text>, found text",
        ),
        (
            "a value function whose only path is a loop",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn f() -> txt { for x in [\"a\"] { return x; }; }; fn main() {};",
            "`f` is declared to return text, so every path must end with `return ...`, `panic;`, or a call that stops the run",
        ),
    ]);
}

#[test]
fn parameterized_vec_rules() {
    expect_accepts(&[
        (
            "a Vec of ints",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn main() { let xs: Vec<int> = [1, 2, 3]; };",
        ),
        (
            "a Vec of text",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn f(a: Vec<txt>) -> Vec<txt> { return a; };\nfn main() {};",
        ),
        (
            "a nested Vec",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn main() { let nested: Vec<Vec<txt>> = [[\"a\"], [\"b\"]]; };",
        ),
        (
            "a Vec of declared types and a loop over it",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nrec Task { name: txt, weight: int };\nfn f(tasks: Vec<Task>) -> int { let n: int = 0; for task in tasks { n = n + task.weight; }; return n; };\nfn main() {};",
        ),
        (
            "a record literal element takes the declared type",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nrec Task { name: txt };\nfn f(tasks: Vec<Task>) {};\nfn main() { f([{ name = \"a\" }]); };",
        ),
    ]);
    expect_rejections(&[
        (
            "a bare Vec has no element type",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn main() { let xs: Vec = []; };",
            "`Vec` takes 1 type argument, found 0",
        ),
        (
            "an element of the wrong type",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn main() { let xs: Vec<int> = [\"a\"]; };",
            "expected int, found text",
        ),
        (
            "a Vec of the wrong element type",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn main() { let xs: Vec<int> = [1]; let ys: Vec<txt> = xs; };",
            "expected Vec<text>, found Vec<int>",
        ),
        (
            "a loop item used as text",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn main() { for x in [1] { std::print(x); }; };",
            "expected text, found int",
        ),
        (
            "a record element missing a declared field",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nrec Task { name: txt };\nfn f(tasks: Vec<Task>) {};\nfn main() { f([{}]); };",
            "`Task` is missing field `name`",
        ),
    ]);
}

#[test]
fn indexing_rules() {
    expect_accepts(&[
        (
            "an element by a literal index",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn f(xs: Vec<int>) -> int { return xs[0]; };\nfn main() {};",
        ),
        (
            "an element by a variable index",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn f(xs: Vec<txt>, i: int) -> txt { return xs[i]; };\nfn main() {};",
        ),
        (
            "a nested index",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn f(xs: Vec<Vec<txt>>) -> txt { return xs[0][1]; };\nfn main() {};",
        ),
    ]);
    expect_rejections(&[
        (
            "indexing a text",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn main() { let s: txt = \"a\"; let c: txt = s[0]; };",
            "expected Vec, found text",
        ),
        (
            "a text index",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn f(xs: Vec<txt>) -> txt { return xs[\"a\"]; };\nfn main() {};",
            "expected int, found text",
        ),
    ]);
}

#[test]
fn enum_rules() {
    expect_accepts(&[
        (
            "an exhaustive enum match",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nenum Color { Red, Green, Blue };\nfn f(c: Color) -> txt { let label: txt = match c { Color.Red => \"r\"; Color.Green => \"g\"; Color.Blue => \"b\"; }; return label; };\nfn main() {};",
        ),
        (
            "a variant pattern binds its payload",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nenum Shape { Dot, Circle(int) };\nfn f(s: Shape) -> int { let label: int = match s { Shape.Circle(r) => r; Shape.Dot => 0; }; return label; };\nfn main() {};",
        ),
        (
            "the default covers the rest",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nenum Color { Red, Green, Blue };\nfn f(c: Color) -> txt { let label: txt = match c { Color.Red => \"r\"; _ => \"other\"; }; return label; };\nfn main() {};",
        ),
        (
            "a statement match may be non-exhaustive",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nenum Color { Red, Green, Blue };\nfn main() { let c: Color = Color.Red; match c { Color.Red => { std::print(\"r\"); }; }; };",
        ),
    ]);
    expect_rejections(&[
        (
            "a missing variant in a value match",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nenum Color { Red, Green, Blue };\nfn f(c: Color) -> txt { let label: txt = match c { Color.Red => \"r\"; }; return label; };\nfn main() {};",
            "a match that produces a value must cover every case; add a `_` arm",
        ),
        (
            "a variant pattern on an unknown enum",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn main() { match \"a\" { Color.Red => {}; _ => {}; }; };",
            "unknown name `Color`",
        ),
        (
            "an unknown variant",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nenum Color { Red };\nfn main() { let c: Color = Color.Red; match c { Color.Blue => {}; _ => {}; }; };",
            "unknown enum variant `Color.Blue`",
        ),
        (
            "a wrong binding count",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nenum Shape { Circle(int) };\nfn main() { let s: Shape = Shape.Circle(1); match s { Shape.Circle(a, b) => {}; }; };",
            "`Circle` takes 1 payload binding(s), found 2",
        ),
        (
            "a wrong construction argument count",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nenum Shape { Circle(int) };\nfn main() { let s: Shape = Shape.Circle(1, 2); };",
            "`Shape.Circle` takes 1 argument, found 2",
        ),
        (
            "a wrong construction argument type",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nenum Shape { Circle(int) };\nfn main() { let s: Shape = Shape.Circle(\"x\"); };",
            "expected int, found text",
        ),
        (
            "a duplicate variant pattern",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nenum Color { Red, Green };\nfn main() { let c: Color = Color.Red; match c { Color.Red => {}; Color.Red => {}; }; };",
            "duplicate match pattern",
        ),
    ]);
}

#[test]
fn generic_type_rules() {
    expect_accepts(&[
        (
            "a generic rec used with arguments",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nrec Pair<A, B> { first: A, second: B };\nfn f(p: Pair<int, txt>) -> int { return p.first; };\nfn main() {};",
        ),
        (
            "a generic enum constructed and matched",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nenum Option<T> { some(T), none };\nfn f(o: Option<int>) -> int { let label: int = match o { Option.some(x) => x; Option.none => 0; }; return label; };\nfn main() {};",
        ),
        (
            "a nested generic type",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nrec Pair<A, B> { first: A, second: B };\nfn f(p: Pair<txt, Pair<int, int>>) -> int { return p.second.second; };\nfn main() {};",
        ),
    ]);
    expect_rejections(&[
        (
            "too few type arguments",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nrec Pair<A, B> { first: A, second: B };\nfn main() { let p: Pair<int> = { first = 1, second = 2 }; };",
            "`Pair` takes 2 type arguments, found 1",
        ),
        (
            "an unknown type parameter",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nrec Bad<T> { x: U };\nfn main() {};",
            "unknown type `U`",
        ),
        (
            "a duplicate type parameter",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nrec Bad<T, T> { x: T };\nfn main() {};",
            "duplicate type parameter `T`",
        ),
        (
            "a payload of the wrong type",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nenum Option<T> { some(T), none };\nfn main() { let o: Option<int> = Option.some(\"x\"); };",
            "expected int, found text",
        ),
        (
            "an uninferrable generic construction",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nenum Option<T> { some(T), none };\nfn main() { match Option.some(1) { _ => {}; }; };",
            "cannot infer the type arguments of `Option`; add a type annotation",
        ),
    ]);
}

#[test]
fn generic_function_rules() {
    expect_accepts(&[
        (
            "a generic identity",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn identity<T>(x: T) -> T { return x; };\nfn main() { let n: int = identity(5); };",
        ),
        (
            "a generic Vec length",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn length<T>(xs: Vec<T>) -> int { let n: int = 0; for item in xs { n = n + 1; }; return n; };\nfn main() { let m: int = length([1, 2, 3]); };",
        ),
        (
            "inference through another generic call",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn id<T>(x: T) -> T { return x; };\nfn wrap<T>(x: T) -> Vec<T> { return [x]; };\nfn main() { let xs: Vec<int> = wrap(id(5)); };",
        ),
    ]);
    expect_rejections(&[
        (
            "conflicting argument types",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn same<T>(a: T, b: T) -> T { return a; };\nfn main() { std::print(same(1, \"x\")); };",
            "`same` cannot take an argument of type text",
        ),
        (
            "an uninferrable type parameter",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn f<T>(n: int) -> int { return n; };\nfn main() { std::print(\"@(f(5))\"); };",
            "cannot infer the type parameter `T` of `f`; add a type annotation",
        ),
    ]);
}

#[test]
fn map_construction_infers_from_the_expected_type() {
    // The expected type at the call site binds the callee's type parameters, so
    // a generic call whose arguments cannot settle them still checks.
    expect_accepts(&[
        (
            "empty infers its type arguments from the binding",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn main() { let m: Map<txt, txt> = std::map::empty(); };",
        ),
        (
            "from infers its type arguments from the binding and the entry literals",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn main() { let m: Map<txt, txt> = std::map::of([{ key = \"a\", value = \"1\" }]); };",
        ),
    ]);
    expect_rejections(&[(
        "empty without a type still cannot infer",
        "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn main() { std::map::empty(); };",
        "cannot infer the type parameter `K` of `std::map::empty`; add a type annotation",
    )]);
}

#[test]
fn private_fields_and_variants_need_pub() {
    // A field and a variant follow the rule a declaration already has: reachable
    // in the declaring namespace, and across namespaces only when `pub`.
    expect_accepts(&[
        (
            "a pub field is reachable across namespaces",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nmod hidden { use std::text; pub rec Boxed { pub shown: txt, hidden: txt }; pub fn boxed() -> Boxed { return { shown = \"s\", hidden = \"h\" }; }; };\nfn main() { let b: hidden::Boxed = hidden::boxed(); std::print(b.shown); };",
        ),
        (
            "a private field is reachable inside its own namespace",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nmod hidden { use std::text; pub rec Boxed { shown: txt }; pub fn boxed() -> Boxed { return { shown = \"s\" }; }; pub fn read(b: Boxed) -> txt { return b.shown; }; };\nfn main() { let b: hidden::Boxed = hidden::boxed(); std::print(hidden::read(b)); };",
        ),
        (
            "a pub variant is constructible across namespaces",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nmod hidden { pub enum Mark { pub shown, hidden }; };\nfn main() { let m: hidden::Mark = hidden::Mark.shown; };",
        ),
        (
            "a private variant is constructible inside its own namespace",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nmod hidden { pub enum Mark { shown }; pub fn mark() -> Mark { return Mark.shown; }; };\nfn main() { let m: hidden::Mark = hidden::mark(); };",
        ),
    ]);
    expect_rejections(&[
        (
            "a private field read across namespaces",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nmod hidden { use std::text; pub rec Boxed { pub shown: txt, hidden: txt }; pub fn boxed() -> Boxed { return { shown = \"s\", hidden = \"h\" }; }; };\nfn main() { let b: hidden::Boxed = hidden::boxed(); std::print(b.hidden); };",
            "`Boxed.hidden` is private to `hidden`; mark it `pub` to reach it",
        ),
        (
            "a private field in a literal across namespaces",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nmod hidden { use std::text; pub rec Boxed { pub shown: txt, hidden: txt }; };\nfn main() { let b: hidden::Boxed = { shown = \"s\", hidden = \"h\" }; };",
            "`Boxed.hidden` is private to `hidden`; mark it `pub` to reach it",
        ),
        (
            "a private variant construction across namespaces",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nmod hidden { pub enum Mark { pub shown, hidden }; };\nfn main() { let m: hidden::Mark = hidden::Mark.hidden; };",
            "`Mark.hidden` is private to `hidden`; mark it `pub` to reach it",
        ),
        (
            "a private variant pattern across namespaces",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nmod hidden { pub enum Mark { pub shown, hidden }; };\nfn f(m: hidden::Mark) -> int { match m { hidden::Mark.hidden => {}; _ => {}; }; return 0; };\nfn main() {};",
            "`Mark.hidden` is private to `hidden`; mark it `pub` to reach it",
        ),
    ]);
}

#[test]
fn option_and_find_rules() {
    expect_accepts(&[
        (
            "an Option value constructed and matched",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn f(o: std::Option<int>) -> int { let label: int = match o { std::Option.some(value) => value; std::Option.none => 0; }; return label; };\nfn main() {};",
        ),
        (
            "find returns an Option and matches on it",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn f(s: txt) -> int { let label: int = match s.find(\"=\") { std::Option.some(index) => index; std::Option.none => -1; }; return label; };\nfn main() {};",
        ),
        (
            "a generic len accepts a Vec of ints",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn main() { let xs: Vec<int> = [1, 2]; let n: int = xs.len(); };",
        ),
    ]);
    expect_rejections(&[(
        "find is not an int",
        "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn f(s: txt) -> int { return s.find(\"=\"); };\nfn main() {};",
        "expected int, found Option<int>",
    )]);
}

#[test]
fn list_assignment_rules() {
    expect_accepts(&[
        (
            "index assignment on a Vec",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn main() { let xs: Vec<int> = [1, 2]; xs[0] = 9; };",
        ),
        (
            "assigning at the length appends",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn main() { let xs: Vec<int> = []; xs[0] = 1; xs[1] = 2; };",
        ),
        (
            "index assignment on a parameter",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn f(xs: Vec<int>) { xs[0] = 1; };\nfn main() {};",
        ),
    ]);
    expect_rejections(&[
        (
            "an index assignment on a non-Vec",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn main() { let n: int = 0; n[0] = 1; };",
            "`n` is not a Vec",
        ),
        (
            "a value of the wrong element type",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn main() { let xs: Vec<int> = []; xs[0] = \"a\"; };",
            "expected int, found text",
        ),
        (
            "a non-int index",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn main() { let xs: Vec<int> = []; xs[\"a\"] = 1; };",
            "expected int, found text",
        ),
    ]);
}

#[test]
fn negative_integer_pattern_rules() {
    expect_accepts(&[(
        "a negative integer match pattern",
        "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn main() { let n: int = -1; match n { -1 => {}; 0 => {}; _ => {}; }; };",
    )]);
    expect_rejections(&[(
        "duplicate negative integer patterns",
        "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn main() { let n: int = -1; match n { -1 => {}; -1 => {}; }; };",
        "duplicate match pattern `-1`",
    )]);
}

#[test]
fn method_rules() {
    expect_accepts(&[
        (
            "a method on a Vec",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn count<T>(self: Vec<T>) -> int { let n: int = 0; for item in self { n = n + 1; }; return n; };\nfn main() { let xs: Vec<int> = [1, 2]; let n: int = xs.count(); };",
        ),
        (
            "a method on a declared type",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nrec Point { x: int, y: int };\nfn sum(self: Point) -> int { return self.x + self.y; };\nfn main() { let p: Point = { x = 1, y = 2 }; let n: int = p.sum(); };",
        ),
        (
            "a method calling an earlier method",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn count<T>(self: Vec<T>) -> int { return 0; }; fn head<T>(self: Vec<T>) -> int { return self.count(); };\nfn main() { let xs: Vec<int> = [1]; let n: int = xs.head(); };",
        ),
        (
            "the receiver parameter's name is irrelevant",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn count<T>(xs: Vec<T>) -> int { return 0; };\nfn main() { let xs: Vec<int> = [1]; let n: int = xs.count(); };",
        ),
    ]);
    expect_rejections(&[
        (
            "an unknown method",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn main() { let xs: Vec<int> = [1, 2]; xs.nope(); };",
            "no method `nope` for this value",
        ),
        (
            "a duplicate name in one namespace",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn count<T>(self: Vec<T>) -> int { return 0; }; fn count<T>(self: Vec<T>) -> int { return 1; };\nfn main() {};",
            "`count` is declared more than once in this namespace",
        ),
        (
            "a free function whose receiver type does not match",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn count(xs: Vec<txt>) -> int { return 0; };\nfn main() { let xs: Vec<int> = [1]; xs.count(); };",
            "expected Vec<text>, found Vec<int>",
        ),
    ]);
}

#[test]
fn closure_rules() {
    expect_accepts(&[
        (
            "an inline closure passed to a function",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn main() { let ys: Vec<int> = [1, 2].map(fn(x: int) -> int { return x * 2; }); };",
        ),
        (
            "a closure stored in a function-typed binding",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn main() { let f: fn(int) -> int = fn(x: int) -> int { return x + 1; }; };",
        ),
        (
            "a closure that captures an enclosing local",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn main() { let factor: int = 3; let f: fn(int) -> int = fn(x: int) -> int { return x * factor; }; };",
        ),
        (
            "a nested closure",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn main() { let base: int = 1; let make: fn(int) -> fn(int) -> int = fn(a: int) -> fn(int) -> int { return fn(b: int) -> int { return base + a + b; }; }; };",
        ),
        (
            "a no-value closure bound and called as a statement",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn main() { let go: fn(txt) = fn(name: txt) { std::print(name); }; go(\"x\"); };",
        ),
        (
            "an async closure call",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn main() { let job: fn() = fn() { std::print(\"x\"); }; async job(); wait; };",
        ),
        (
            "a closure that captures a loop variable",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn main() { for x in [1, 2] { let f: fn() -> int = fn() -> int { return x; }; }; };",
        ),
    ]);
    expect_rejections(&[
        (
            "a closure used where a value is expected",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn main() { let n: int = fn(x: int) -> int { return x; }; };",
            "expected int, found fn(int) -> int",
        ),
        (
            "assigning to a captured local",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn main() { let total: int = 0; let bump: fn(int) -> int = fn(x: int) -> int { total = total + x; return total; }; };",
            "`total` is captured by a closure and cannot be assigned",
        ),
        (
            "a closure body that does not return",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn main() { let f: fn(int) -> int = fn(x: int) -> int { let y: int = x; }; };",
            "`<closure>` is declared to return int, so every path must end with `return ...`, `panic;`, or a call that stops the run",
        ),
        (
            "a wrong-typed closure return",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn main() { let f: fn(int) -> int = fn(x: int) -> int { return \"s\"; }; };",
            "expected int, found text",
        ),
    ]);
}

#[test]
fn function_value_rules() {
    expect_accepts(&[
        (
            "a function passed as a value",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn double(x: int) -> int { return x * 2; };\nfn apply(f: fn(int) -> int, x: int) -> int { return f(x); };\nfn main() { let n: int = apply(double, 5); };",
        ),
        (
            "a function bound to a value",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn double(x: int) -> int { return x * 2; };\nfn main() { let g: fn(int) -> int = double; let n: int = g(5); };",
        ),
        (
            "a generic map infers through the function argument",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn double(x: int) -> int { return x * 2; };\nfn main() { let xs: Vec<int> = [1, 2]; let ys: Vec<int> = xs.map(double); };",
        ),
    ]);
    expect_rejections(&[
        (
            "a function used where a value is expected",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn double(x: int) -> int { return x * 2; };\nfn main() { let n: int = double; };",
            "expected int, found fn(int) -> int",
        ),
        (
            "calling a non-function value",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn main() { let x: txt = \"a\"; x(); };",
            "`x` is a value and cannot be called",
        ),
        (
            "an indirect call of the wrong arity",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn double(x: int) -> int { return x * 2; };\nfn main() { let g: fn(int) -> int = double; let n: int = g(1, 2); };",
            "a function value takes 1 argument, found 2",
        ),
        (
            "a generic function as a value",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn id<T>(x: T) -> T { return x; };\nfn main() { let g: fn(int) -> int = id; };",
            "`id` is generic; give it a type annotation to use it as a value",
        ),
    ]);
}

#[test]
fn main_returns_int_or_nothing() {
    expect_accepts(&[
        (
            "main returning int",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn main() -> int { return 0; };",
        ),
        (
            "main returning nothing",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn main() {};",
        ),
    ]);
    expect_rejections(&[
        (
            "main returning text",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn main() -> txt { return \"0\"; };",
            "`main` returns int or nothing",
        ),
        (
            "main returning a Vec",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn main() -> Vec<txt> { return []; };",
            "`main` returns int or nothing",
        ),
    ]);
}

/// The audit's rule-conformance probes, visibility: Every declaration, field, and variant is private by default; `pub` reaches it across namespaces, and `std` is reserved.
///
/// Each case is the smallest program for one stated rule. The sources are the
/// audit probes under `/tmp/probes` made self-contained (each imports what it names),
/// so a rule that stops being enforced fails here.
#[test]
fn rule_conformance_visibility() {
    expect_accepts(&[
        (
            "a public function is reachable across namespaces",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nmod m { use std; pub fn shown() -> int { return 1; }; };\nfn main() { std::print(\"@(m::shown())\"); };\n",
        ),
        (
            "a public field is reachable across namespaces",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nmod m {\n  use std; use std::text;\n  pub rec Entry { pub key: txt };\n  pub let e: Entry = { key = \"k\" };\n};\nfn main() { std::print(m::e.key); };\n",
        ),
        (
            "a public variant is constructible across namespaces",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nmod m { use std; pub enum Colour { pub red }; };\nfn main() { let c: m::Colour = m::Colour.red; };\n",
        ),
        (
            "a public method is reachable through `use`",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nmod m { use std; use std::text; pub fn shout(self: txt) -> txt { return self; }; };\nuse m;\nfn main() { let s: txt = \"a\".shout(); std::print(s); };\n",
        ),
    ]);
    expect_rejections(&[
        (
            "a private function is unreachable across namespaces",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nmod m { use std; fn hidden() -> int { return 1; }; };\nfn main() { std::print(\"@(m::hidden())\"); };\n",
            "unknown name `m::hidden`",
        ),
        (
            "a private field is unreachable across namespaces",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nmod m {\n  use std; use std::text;\n  pub rec Entry { key: txt };\n  pub let e: Entry = { key = \"k\" };\n};\nfn main() { std::print(m::e.key); };\n",
            "`Entry.key` is private to `m`; mark it `pub` to reach it",
        ),
        (
            "a private variant is unconstructible across namespaces",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nmod m { use std; pub enum Colour { red }; };\nfn main() { let c: m::Colour = m::Colour.red; };\n",
            "`Colour.red` is private to `m`; mark it `pub` to reach it",
        ),
        (
            "a private type is unreachable across namespaces",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nmod m { use std; rec P { x: int }; };\nfn main() { let p: m::P = { x = 1 }; };\n",
            "unknown type `m::P`",
        ),
        (
            "a private method is unreachable across namespaces",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nmod m { use std; use std::text; fn shout(self: txt) -> txt { return self; }; };\nfn main() { let s: txt = \"a\".shout(); std::print(s); };\n",
            "no method `shout` for this value",
        ),
        (
            "a rooted path cannot reach a private name",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nmod m { use std; let hidden: int = 1; };\nfn main() { std::print(\"@(::hidden)\"); };\n",
            "unknown name `::hidden`",
        ),
        (
            "`std` is reserved and cannot be declared",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nmod std { pub fn f() -> int { return 1; }; };\nfn main() {};\n",
            "`std` is reserved and cannot be declared",
        ),
    ]);
}

/// The audit's rule-conformance probes, order: A name is declared before use, a name may not reference itself, one pool of names admits only methods that differ by receiver head, and a type may be recursive.
///
/// Each case is the smallest program for one stated rule. The sources are the
/// audit probes under `/tmp/probes` made self-contained (each imports what it names),
/// so a rule that stops being enforced fails here.
#[test]
fn rule_conformance_order_and_scope() {
    expect_accepts(&[
        (
            "a local shadows a top-level value",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nlet a: int = 1;\nfn main() { let a: int = 2; std::print(\"@(a)\"); };\n",
        ),
        (
            "a method name repeats once per receiver head",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nrec A { x: int };\nrec B { y: int };\nfn about(self: A) -> int { return 1; };\nfn about(self: B) -> int { return 2; };\nfn main() { std::print(\"ok\"); };\n",
        ),
        (
            "a method call dispatches on the receiver's type",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn main() {\n  let xs: Vec<txt> = [\"a\",\"b\",\"c\"];\n  let n: int = xs.len();\n  let m: int = \"abc\".len();\n  std::print(\"@(n) @(m)\");\n};\n",
        ),
        (
            "a recursive declared type",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nrec Tree { name: txt, kids: Vec<Tree> };\nfn main() { let t: Tree = { name = \"r\", kids = [] }; std::print(t.name); };\n",
        ),
    ]);
    expect_rejections(&[
        (
            "a forward reference",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn main() { std::print(banner()); };\nfn banner() -> txt { return \"x\"; };\n",
            "`banner` is declared after this point",
        ),
        (
            "a function that calls itself",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn f() -> int { return f(); };\nfn main() { std::print(\"@(f())\"); };\n",
            "a name cannot reference itself",
        ),
        (
            "a value that references itself",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nlet x: int = x;\nfn main() { std::print(\"@(x)\"); };\n",
            "a name cannot reference itself",
        ),
        (
            "a duplicate local",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn main() { let a: int = 1; let a: int = 2; std::print(\"@(a)\"); };\n",
            "`a` is declared more than once",
        ),
        (
            "a value and a function share a name",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nlet thing: txt = \"v\";\nfn thing() -> txt { return \"f\"; };\nfn main() {};\n",
            "`thing` is declared more than once in this namespace",
        ),
        (
            "two methods for one receiver head",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn len(self: txt) -> int { return 1; };\nfn len(self: txt) -> int { return 2; };\nfn main() { std::print(\"a\"); };\n",
            "`len` is declared more than once in this namespace",
        ),
        (
            "a method declared after a head-less function",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nrec A { x: int };\nfn m() -> int { return 1; };\nfn m(self: A) -> int { return 2; };\nfn main() {};\n",
            "`m` is declared more than once in this namespace",
        ),
        (
            "a method declared after a module value",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nrec A { x: int };\nlet m: int = 1;\nfn m(self: A) -> int { return 2; };\nfn main() {};\n",
            "`m` is declared more than once in this namespace",
        ),
        (
            "a module value declared after a method",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nrec A { x: int };\nfn m(self: A) -> int { return 2; };\nlet m: int = 1;\nfn main() {};\n",
            "`m` is declared more than once in this namespace",
        ),
    ]);
}

/// The audit's rule-conformance probes, declarations: A bodyless `type` names a runtime type in a `std` category; fields, variants, type parameters, and parameters are unique; a generic type is applied; and `main` has its one shape.
///
/// Each case is the smallest program for one stated rule. The sources are the
/// audit probes under `/tmp/probes` made self-contained (each imports what it names),
/// so a rule that stops being enforced fails here.
#[test]
fn rule_conformance_declarations() {
    expect_accepts(&[
        (
            "a built-in type name is an ordinary name",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn txt() -> int { return 1; };\nfn main() { std::print(\"@(txt())\"); };\n",
        ),
        (
            "`Vec` and `map` are ordinary names",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn Vec() -> int { return 1; };\nfn map() -> int { return 2; };\nfn main() { std::print(\"@(Vec()) @(map())\"); };\n",
        ),
        (
            "the intrinsics need no `use`",
            "fn main() { let x: int = 1; let s: txt = \"a\"; let b: bool = true; let xs: Vec<int> = [1]; std::print(\"@(x)@(s)@(b)\"); };\n",
        ),
    ]);
    expect_rejections(&[
        (
            "a bodyless declaration that is not a built-in",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\ntype Foo;\nfn main() {};\n",
            "`Foo` is not a type the runtime provides; a declaration with no body must name a built-in type",
        ),
        (
            "a bodyless built-in outside a `std` category",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\ntype txt;\nfn main() {};\n",
            "`txt` is a type the runtime provides; a declaration with no body must be in a `std` category",
        ),
        (
            "a duplicate field in a declaration",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nrec P { x: int, x: int };\nfn main() {};\n",
            "duplicate field `x`",
        ),
        (
            "a duplicate field in a literal",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nrec P { x: int };\nfn main() { let p: P = { x = 1, x = 2 }; };\n",
            "duplicate field `x`",
        ),
        (
            "a duplicate enum variant",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nenum C { red, red };\nfn main() {};\n",
            "duplicate variant `red`",
        ),
        (
            "a duplicate type parameter on a `rec`",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nrec P<A, A> { first: A };\nfn main() {};\n",
            "duplicate type parameter `A`",
        ),
        (
            "a duplicate type parameter on a function",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn f<A, A>(x: A) -> int { return 0; };\nfn main() {};\n",
            "duplicate type parameter `A`",
        ),
        (
            "a generic type used without arguments",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nrec Pair<A, B> { first: A, second: B };\nfn main() { let p: Pair = { first = 1, second = 2 }; };\n",
            "`Pair` takes 2 type arguments, found 0",
        ),
        (
            "a generic type with too few arguments",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nrec Pair<A, B> { first: A, second: B };\nfn main() { let p: Pair<int> = { first = 1, second = 2 }; };\n",
            "`Pair` takes 2 type arguments, found 1",
        ),
        (
            "`=` in a type declaration",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nrec P = { x: int };\nfn main() {};\n",
            "expected `{` to open the type, found `=`",
        ),
        (
            "a type name on a literal",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nrec P { x: int };\nfn main() { let p: P = P { x = 1 }; };\n",
            "expected `;` after the initializer, found `{`",
        ),
        (
            "`main` with a non-`Vec<txt>` parameter",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn main(x: int) {};\n",
            "`main`'s parameter must be declared `Vec<txt>`",
        ),
        (
            "`main` returning text",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn main() -> txt { return \"x\"; };\n",
            "`main` returns int or nothing",
        ),
        (
            "`main` declared only inside a module",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nmod app { pub fn main() {}; };\n",
            "the program has no root `main` function",
        ),
        (
            "`main` with two parameters",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn main(a: Vec<txt>, b: Vec<txt>) {};\n",
            "`main` takes at most one parameter",
        ),
        (
            "assigning a module-level value",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nlet x: int = 1;\nfn main() { x = 2; };\n",
            "`x` is a module-level constant and cannot be assigned",
        ),
        (
            "a bare literal statement",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn main() { \"x\"; };\n",
            "a statement must be a call",
        ),
        (
            "a no-value call bound as a value",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn nothing() {};\nfn main() { let x: int = nothing(); };\n",
            "expected int, found no value",
        ),
        (
            "a duplicate parameter",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn f(a: int, a: int) {};\nfn main() {};\n",
            "`a` is declared more than once",
        ),
        (
            "`Map` without `use std::map;`",
            "use std;\nfn main() { let m: Map<txt, txt> = {}; };\n",
            "unknown type `Map`",
        ),
    ]);
}

/// The audit's rule-conformance probes, match: A value match covers every case, a `_` arm is last, patterns are literals, names, or field paths, and a variant pattern matches its own subject.
///
/// Each case is the smallest program for one stated rule. The sources are the
/// audit probes under `/tmp/probes` made self-contained (each imports what it names),
/// so a rule that stops being enforced fails here.
#[test]
fn rule_conformance_match() {
    expect_accepts(&[
        (
            "a bare statement match",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn main() { match \"a\" { \"a\" => { std::print(\"hit\"); }; _ => {}; }; };\n",
        ),
        (
            "an exhaustive value match",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nenum C { red, green };\nfn main() { let c: C = C.red; let x: int = match c { C.red => 1; C.green => 2; }; std::print(\"@(x)\"); };\n",
        ),
        (
            "a boolean match covering both cases",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn main() { let f: bool = true; let x: int = match f { true => 1; false => 2; }; std::print(\"@(x)\"); };\n",
        ),
        (
            "a statement match with no default",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn main() { match \"a\" { \"a\" => { std::print(\"a\"); }; }; };\n",
        ),
    ]);
    expect_rejections(&[
        (
            "a non-exhaustive value match",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nenum C { red, green };\nfn main() { let c: C = C.red; let x: int = match c { C.red => 1; }; };\n",
            "a match that produces a value must cover every case; add a `_` arm",
        ),
        (
            "a variant pattern against the wrong subject",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nenum C { red };\nenum D { blue };\nfn main() { let c: C = C.red; let x: int = match c { D.blue => 1; _ => 2; }; };\n",
            "expected C, found D",
        ),
        (
            "a duplicate match arm",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn main() { let x: int = match \"a\" { \"a\" => 1; \"a\" => 2; _ => 3; }; };\n",
            "duplicate match pattern `a`",
        ),
        (
            "a `_` arm that is not last",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn main() { match \"a\" { _ => {}; \"x\" => {}; }; };\n",
            "the `_` arm must be last",
        ),
        (
            "an operator in a match pattern",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn main() { let x: int = match 3 { 1 + 2 => 1; _ => 0; }; };\n",
            "a match arm is a literal, a name, or a field path",
        ),
    ]);
}

/// The audit's rule-conformance probes, syntax: Reserved words are the forms, not type names; `import` is top-level, `mut`/`if`/slice/`call()` do not exist; `+` is arithmetic; and `@(…)` inserts only text, an integer, or a boolean.
///
/// Each case is the smallest program for one stated rule. The sources are the
/// audit probes under `/tmp/probes` made self-contained (each imports what it names),
/// so a rule that stops being enforced fails here.
#[test]
fn rule_conformance_syntax() {
    expect_accepts(&[
        (
            "`use` inside a module",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nmod m { use std; use std::text; pub fn go() { std::print(\"x\"); }; };\nfn main() { m::go(); };\n",
        ),
        (
            "`&&` short-circuits",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn boom() -> bool { std::print(\"boom\"); return true; };\nfn main() { let r: bool = false && boom(); std::print(\"@(r)\"); };\n",
        ),
        (
            "the sign of `%` follows the dividend",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn main() { std::print(\"@(-7 % 3) @(7 % -3)\"); };\n",
        ),
        (
            "a binding is assignable without `mut`",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn main() { let x: int = 1; x = 2; std::print(\"@(x)\"); };\n",
        ),
        (
            "`Option` is reached through `use std;`",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn main() { let o: Option<int> = Option.none; };\n",
        ),
        (
            "a reserved word is usable as a name with a suffix",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn main() { let if_: int = 1; std::print(\"@(if_)\"); };\n",
        ),
    ]);
    expect_rejections(&[
        (
            "`import` inside a module",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nmod m { import \"other.kiru\"; };\nfn main() {};\n",
            "expected a declaration, found `import`",
        ),
        (
            "a reserved word as a binding name",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn main() { let match: int = 1; };\n",
            "expected a name in a binding, found `match`",
        ),
        (
            "`if` is not a form",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn main() { if true { std::print(\"x\"); }; };\n",
            "expected `;` after the expression, found `true`",
        ),
        (
            "a bare `Vec` type",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn main() { let xs: Vec = []; };\n",
            "`Vec` takes 1 type argument, found 0",
        ),
        (
            "slice syntax",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn main() { let xs: Vec<int> = [1,2,3]; let ys: Vec<int> = xs[0..2]; };\n",
            "expected `]` to close an index, found `..`",
        ),
        (
            "a call of a call",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn make() -> fn() -> int { return fn() -> int { return 1; }; };\nfn main() { let x: int = make()(); };\n",
            "expected `;` after the initializer, found `(`",
        ),
        (
            "text concatenation with `+`",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn main() { let s: txt = \"a\" + \"b\"; };\n",
            "expected int, found text",
        ),
        (
            "`mut` is not a keyword",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn main() { let mut x: int = 1; };\n",
            "expected `:` in a binding, found `x`",
        ),
        (
            "a Vec in `@(…)`",
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn main() { let xs: Vec<int> = [1,2]; std::print(\"@(xs)\"); };\n",
            "`@(…)` inserts text, an integer, or a boolean, found Vec<int>",
        ),
        (
            "`Option` without `use std;`",
            "use std::text;\nuse std::vec;\nuse std::map;\nfn main() { let o: Option<int> = Option.none; };\n",
            "unknown name `Option`",
        ),
    ]);
}
