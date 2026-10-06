use crate::compiler::{checked_files, checked_program};
use crate::model::{DeclarationId, Kind, Program};

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
        .value(name)
        .or_else(|| program.namespace(namespace).function(name))
        .expect("the name exists")
}

/// The first parameter of a callable declaration.
fn first_parameter(program: &Program, id: DeclarationId) -> DeclarationId {
    program.declaration(id).parameters[0]
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
        ("main without a parameter", "fn main() {};"),
        ("main with a record parameter", "fn main(rec args) {};"),
        (
            "function without a return",
            "fn f() { txt x = \"a\"; };\nfn main() {};",
        ),
        (
            "function with a return",
            "fn f() -> txt { return(\"a\"); };\nfn main() {};",
        ),
        (
            "an early return in a case arm with a final return",
            "fn f(txt s) -> txt { switch(s) { case(\"a\") { return(\"a\"); }; }; return(\"\"); };\nfn main() {};",
        ),
        (
            "unreachable code after a return",
            "fn f() -> txt { return(\"a\"); return(\"b\"); };\nfn main() {};",
        ),
        ("a bare return in main", "fn main() { return(); };"),
        (
            "a value-returning main",
            "fn main() -> txt { return(\"\"); };",
        ),
        (
            "a value function that panics on every other path",
            "fn f(txt s) -> txt { switch(s) { case(\"a\") { return(\"a\"); }; default { panic; }; }; };\nfn main() {};",
        ),
        (
            "a value function that panics on every path",
            "fn f() -> txt { panic; };\nfn main() {};",
        ),
        (
            "a value function that ends in a halting call",
            "fn f() -> txt { std::io::eprint(\"x\"); };\nfn main() {};",
        ),
        (
            "a value function that ends in a halting user function",
            "fn stop() { panic; };\nfn f() -> txt { stop(); };\nfn main() {};",
        ),
        (
            "halting is transitive",
            "fn inner() { panic; };\nfn outer() { inner(); };\nfn f() -> txt { outer(); };\nfn main() {};",
        ),
    ]);
    expect_rejections(&[
        (
            "a value function that can fall through",
            "fn f(txt s) -> txt { switch(s) { case(\"a\") { return(\"a\"); }; }; };\nfn main() {};",
            "`f` is declared to return text, so every path must end with `return(...)`, `panic;`, or a call that stops the run",
        ),
        (
            "a returned value of the wrong kind",
            "fn f(txt s) -> txt { switch(s) { case(\"a\") { return(\"a\"); }; default { return({ k = \"v\" }); }; }; };\nfn main() {};",
            "expected text, found record",
        ),
        (
            "returning a nothing call",
            "fn work() {};\nfn f() -> txt { return(work()); };\nfn main() {};",
            "expected text, found nothing",
        ),
        (
            "a value return in a nothing function",
            "fn f() { return(\"a\"); };\nfn main() {};",
            "`f` is declared to return nothing, so `return` cannot carry a value",
        ),
        (
            "a bare return in a value function",
            "fn f() -> txt { return(); };\nfn main() {};",
            "`f` is declared to return text, so `return` must carry a value",
        ),
    ]);
}

#[test]
fn panic_and_void_rules() {
    expect_accepts(&[("panic as a statement", "fn main() { panic; };")]);
    expect_rejections(&[
        (
            "eprint bound to text",
            "fn main() { txt x = std::io::eprint(\"x\"); };",
            "expected text, found nothing",
        ),
        (
            "eprint passed as a text argument",
            "fn take(txt x) {};\nfn main() { take(std::io::eprint(\"x\")); };",
            "expected text, found nothing",
        ),
        (
            "eprint in a record field",
            "fn main() { rec r = { k = std::io::eprint(\"x\") }; };",
            "expected text, found nothing",
        ),
    ]);
}

#[test]
fn command_and_field_rules() {
    expect_accepts(&[
        (
            "command code returned through a function",
            "fn code(txt line) -> txt { return(std::process::command({}, line)); };\nfn main() { txt code = code(\"echo hi\"); };",
        ),
        (
            "command at a module value",
            "txt x = std::process::command({}, \"echo hi\");\nfn main() {};",
        ),
        (
            "command inside a record",
            "rec r = { code = std::process::command({}, \"echo hi\") };\nfn main() {};",
        ),
        (
            "command as a bare statement",
            "fn main() { std::process::command({}, \"true\"); };",
        ),
    ]);
    expect_rejections(&[
        (
            "command bound to a record",
            "fn main() { rec x = std::process::command({}, \"ls\"); };",
            "expected record, found text",
        ),
        (
            "command passed as a record argument",
            "fn take(rec x) {};\nfn main() { take(std::process::command({}, \"ls\")); };",
            "expected record, found text",
        ),
        (
            "command without arguments",
            "fn main() { std::process::command(); };",
            "`std::process::command` takes 2 arguments, found 0",
        ),
        (
            "command with one argument",
            "fn main() { std::process::command({}); };",
            "`std::process::command` takes 2 arguments, found 1",
        ),
        (
            "command with a text spec",
            "fn main() { std::process::command(\"ls\", \"\"); };",
            "expected record, found text",
        ),
        (
            "field access on text",
            "fn main() { txt x = \"a\".b; };",
            "expected record, found text",
        ),
    ]);
}

#[test]
fn text_and_record_rules() {
    expect_accepts(&[
        (
            "record argument",
            "fn read(rec repo) -> txt { return(repo.dir); };\nfn main() { read({ dir = \"/x\" }); };",
        ),
        (
            "record parameter through a call",
            "rec backend = { dir = \"/b\" };\nfn read_it(rec repo) -> txt { return(repo.dir); };\nfn main() { read_it(backend); };",
        ),
        (
            "text parameter through a call",
            "fn identity(txt value) -> txt { return(value); };\nfn main() { std::io::print(identity(\"x\")); };",
        ),
        (
            "record parameter through a call with a literal",
            "fn identity(rec repo) -> txt { return(repo.dir); };\nfn main() { std::io::print(identity({ dir = \"/x\" })); };",
        ),
    ]);
    expect_rejections(&[
        (
            "plus on a record",
            "fn main() { txt x = \"a\" + { b = \"c\" }; };",
            "expected text, found record",
        ),
        (
            "text parameter used as a record",
            "fn f(txt x) { txt a = x; txt b = x.b; };\nfn main() { f(\"s\"); };",
            "expected record, found text",
        ),
        (
            "text where a record is required",
            "fn read(rec repo) -> txt { return(repo.dir); };\nfn main() { read(\"x\"); };",
            "expected record, found text",
        ),
    ]);
}

#[test]
fn async_wait_rules() {
    expect_accepts(&[
        (
            "nothing function as a statement and an async spawn",
            "fn work() {};\nfn main() {\nwork();\nasync work();\nwait;\n};",
        ),
        (
            "async of a command call",
            "fn main() { async std::process::command({}, \"true\"); };",
        ),
        (
            "async of a call returning text",
            "fn pick() -> txt { return(\"x\"); };\nfn main() { async pick(); wait; };",
        ),
        (
            "async of a halting call does not stop the caller",
            "fn stop() { panic; };\nfn main() { async stop(); };",
        ),
        (
            "bare async statement",
            "fn work() {};\nfn main() { async work(); };",
        ),
        (
            "wait as a statement",
            "fn work() {};\nfn main() { async work(); wait; };",
        ),
    ]);
    expect_rejections(&[
        (
            "async of a non-call",
            "fn main() { async \"work\"; };",
            "`async` takes a call",
        ),
        (
            "async of a text variable",
            "txt x = \"a\";\nfn main() { async x; };",
            "`async` takes a call",
        ),
    ]);
}

#[test]
fn bindings_and_scope_rules() {
    expect_accepts(&[(
        "literal, name, and field-path case arms",
        "rec spec = { cmd = \"a\" };\nfn main() {\ntxt a = \"a\";\nswitch(\"a\") { case(\"a\") {}; case(a) {}; case(spec.cmd) {}; };\n};",
    )]);
    expect_rejections(&[
        (
            "parameter assignment",
            "fn f(txt x) { x = \"a\"; };\nfn main() {};",
            "`x` is not mutable; declare it `mut`",
        ),
        (
            "module assignment",
            "txt x = \"a\";\nfn main() { x = \"b\"; };",
            "`x` is a module-level constant and cannot be assigned",
        ),
        (
            "assigning a record to a text local",
            "fn main() { mut txt x = \"a\"; x = {}; };",
            "expected text, found record",
        ),
        (
            "duplicate literal case pattern",
            "fn main() { switch(\"a\") { case(\"a\") {}; case(\"a\") {}; }; };",
            "duplicate case pattern `a`",
        ),
        (
            "record case pattern",
            "fn main() { switch(\"a\") { case({}) {}; }; };",
            "a case arm is a literal, a name, or a field path",
        ),
        (
            "case arm does not leak locals",
            "fn main() {\nswitch(\"a\") {\ncase(\"a\") { txt hidden = \"x\"; };\ncase(\"b\") { txt seen = hidden; };\n};\n};",
            "unknown name `hidden`",
        ),
        (
            "a call case arm",
            "fn pick() -> txt { return(\"a\"); };\nfn main() {\nswitch(\"a\") { case(pick()) {}; };\n};",
            "a case arm is a literal, a name, or a field path",
        ),
        (
            "a concatenation case arm",
            "fn main() {\nswitch(\"ab\") { case(\"a\" + \"b\") {}; };\n};",
            "a case arm is a literal, a name, or a field path",
        ),
        (
            "duplicate reference case patterns",
            "fn main() {\ntxt a = \"a\";\nswitch(\"a\") { case(a) {}; case(a) {}; };\n};",
            "duplicate case pattern",
        ),
    ]);
}

#[test]
fn name_and_call_rules() {
    expect_accepts(&[
        (
            "root access when a local shadows the root",
            "txt value = \"root\";\nfn main() {\ntxt value = \"local\";\nstd::io::print(value);\nstd::io::print(::value);\n};",
        ),
        (
            "a value and a function share a name",
            "fn build() -> txt { return(\"built\"); };\ntxt build = \"text\";\nfn main() { std::io::print(build); std::io::print(build()); };",
        ),
    ]);
    expect_rejections(&[
        (
            "function as a value",
            "fn f() {};\nfn main() { txt x = f; };",
            "`f` is a function; call it",
        ),
        (
            "calling a module value",
            "txt x = \"a\";\nfn main() { x(); };",
            "`x` is a value and cannot be called",
        ),
        (
            "calling a local",
            "fn main() { txt x = \"a\"; x(); };",
            "unknown name `x`",
        ),
        (
            "wrong arity",
            "fn f(txt a) {};\nfn main() { f(); };",
            "`f` takes 1 arguments, found 0",
        ),
        (
            "unknown rooted name",
            "txt value = \"root\";\nfn main() { std::io::print(::missing); };",
            "unknown name `::missing`",
        ),
        (
            "function self reference",
            "fn f() { f(); };\nfn main() {};",
            "a function cannot reference itself",
        ),
        (
            "value self reference",
            "txt x = x;\nfn main() {};",
            "a value cannot reference itself",
        ),
    ]);
}

#[test]
fn statement_form_rules() {
    expect_accepts(&[(
        "bare call and panic statements",
        "fn work() {};\nfn main() {\nwork();\nstd::process::command({}, \"true\");\npanic;\n};",
    )]);
    expect_rejections(&[
        (
            "bare literal statement",
            "fn main() { \"x\"; };",
            "a statement must be a call",
        ),
        (
            "bare reference statement",
            "fn main() { txt name = \"x\"; name; };",
            "a statement must be a call",
        ),
        (
            "bare record statement",
            "fn main() { { k = \"v\" }; };",
            "a statement must be a call",
        ),
        (
            "bare add statement",
            "fn main() { \"a\" + \"b\"; };",
            "a statement must be a call",
        ),
    ]);
}

#[test]
fn mutability_rules() {
    expect_accepts(&[
        (
            "rebind a mut text",
            "fn f() { mut txt x = \"a\"; x = \"b\"; };\nfn main() {};",
        ),
        (
            "field assign a mut record",
            "fn f() { mut rec r = { a = \"1\" }; r.a = \"2\"; };\nfn main() {};",
        ),
        (
            "field assign adds a field",
            "fn f() { mut rec r = { a = \"1\" }; r.b = \"2\"; };\nfn main() {};",
        ),
        (
            "rebind a mut parameter",
            "fn f(mut txt x) { x = \"b\"; };\nfn main() {};",
        ),
        (
            "field assign a mut parameter",
            "fn f(mut rec r) { r.a = \"b\"; };\nfn main() {};",
        ),
    ]);
    expect_rejections(&[
        (
            "rebind an immutable local",
            "fn f() { txt x = \"a\"; x = \"b\"; };\nfn main() {};",
            "`x` is not mutable; declare it `mut`",
        ),
        (
            "field assign an immutable record",
            "fn f() { rec r = { a = \"1\" }; r.a = \"2\"; };\nfn main() {};",
            "`r` is not mutable; declare it `mut`",
        ),
        (
            "field assign an immutable parameter",
            "fn f(txt x) { x.a = \"b\"; };\nfn main() {};",
            "`x` is not mutable; declare it `mut`",
        ),
        (
            "field assign a text binding",
            "fn f() { mut txt x = \"a\"; x.b = \"c\"; };\nfn main() {};",
            "`x` is not a record and has no fields",
        ),
        (
            "field assign a module value",
            "rec r = { a = \"1\" };\nfn main() { r.a = \"2\"; };",
            "`r` is a module-level constant and cannot be assigned",
        ),
    ]);
}

#[test]
fn checks_the_shipped_library_kinds() {
    let program = checked_program(
        "rec backend = { dir = \"/kiru\" };\n\
         fn read(rec repo) -> txt { return(repo.dir); };\n\
         fn main() {\n\
           std::io::print(\"x\");\n\
           read(backend);\n\
         };",
    );

    let command = declaration(&program, &["std", "process"], "command");
    assert_eq!(program.declaration(command).derived.kind, Some(Kind::Text));

    let print = declaration(&program, &["std", "io"], "print");
    assert_eq!(
        program
            .declaration(first_parameter(&program, print))
            .derived
            .kind,
        Some(Kind::Text)
    );
    assert_eq!(program.declaration(print).derived.kind, Some(Kind::Nothing));

    let eprint = declaration(&program, &["std", "io"], "eprint");
    assert_eq!(
        program.declaration(eprint).derived.kind,
        Some(Kind::Nothing)
    );
    assert!(
        program.declaration(eprint).derived.halts,
        "eprint ends in `panic;`, so it stops the run"
    );

    let read = declaration(&program, &[], "read");
    assert_eq!(
        program
            .declaration(first_parameter(&program, read))
            .derived
            .kind,
        Some(Kind::Record)
    );
}

#[test]
fn list_and_loop_rules() {
    expect_accepts(&[
        (
            "a list literal binding and a for loop",
            "fn main() { list xs = [\"a\", \"b\"]; for x in xs { std::io::print(x); }; };",
        ),
        (
            "a function that takes and returns a list",
            "fn id(list xs) -> list { return(xs); }; fn main() { id([\"a\"]); };",
        ),
        (
            "a module-level list value",
            "list xs = [\"a\"]; fn main() { for x in xs { std::io::print(x); }; };",
        ),
        (
            "break inside a for",
            "fn main() { for x in [\"a\"] { break; }; };",
        ),
        (
            "a for that skips an element with a switch default",
            "fn main() { for x in [\"a\"] { switch(x) { case(\"a\") {}; default { std::io::print(x); }; }; }; };",
        ),
        (
            "an unbounded for that breaks",
            "fn main() { for { break; }; };",
        ),
        (
            "an unbounded for as the only path of a value function",
            "fn f() -> txt { for { std::io::print(\"a\"); }; }; fn main() {};",
        ),
    ]);
    expect_rejections(&[
        (
            "for over text",
            "fn main() { for x in \"a\" { std::io::print(x); }; };",
            "expected list, found text",
        ),
        (
            "break outside a loop",
            "fn main() { break; };",
            "`break` is only allowed inside a loop",
        ),
        (
            "a list element that is not text",
            "fn main() { list xs = [{ k = \"v\" }]; };",
            "expected text, found record",
        ),
        (
            "a list binding whose value is not a list",
            "fn main() { list xs = \"a\"; };",
            "expected list, found text",
        ),
        (
            "a value function whose only path is a loop",
            "fn f() -> txt { for x in [\"a\"] { return(x); }; }; fn main() {};",
            "`f` is declared to return text, so every path must end with `return(...)`, `panic;`, or a call that stops the run",
        ),
    ]);
}

#[test]
fn main_returns_text_or_nothing() {
    expect_accepts(&[
        (
            "main returning text",
            "fn main() -> txt { return(\"0\"); };",
        ),
        ("main returning nothing", "fn main() {};"),
    ]);
    expect_rejections(&[
        (
            "main returning a record",
            "fn main() -> rec { return({}); };",
            "`main` returns text or nothing",
        ),
        (
            "main returning a list",
            "fn main() -> list { return([]); };",
            "`main` returns text or nothing",
        ),
    ]);
}
