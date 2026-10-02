use super::analyze;
use crate::compiler::{DeclarationId, DeclarationKind, Kind, Program};
use crate::compiler::{link, loader};

fn check(files: &[(&str, &str)], entry: &str) -> Result<(), String> {
    let directory = tempfile::tempdir().expect("temp dir");
    for (name, source) in files {
        let path = directory.path().join(name);
        std::fs::write(&path, source).expect("write file");
    }
    let mut loaded = loader::load(&directory.path().join(entry)).expect("loads");
    let mut program = link::link(&mut loaded).map_err(|diagnostic| diagnostic.message)?;
    analyze(&mut program).map_err(|diagnostic| diagnostic.message)
}

/// Load, link, and check one source file, returning the checked program.
fn checked(source: &str) -> Program {
    let directory = tempfile::tempdir().expect("temp dir");
    let path = directory.path().join("main.kiru");
    std::fs::write(&path, source).expect("write file");
    let mut loaded = loader::load(&path).expect("loads");
    let mut program = link::link(&mut loaded).expect("links");
    analyze(&mut program).expect("checks");
    program
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

/// The first parameter of a function declaration.
fn first_parameter(program: &Program, id: DeclarationId) -> DeclarationId {
    let DeclarationKind::Function(function) = &program.declaration(id).kind else {
        panic!("expected a function");
    };
    function.parameters[0]
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
            "fn f() { return \"a\"; };\nfn main() {};",
        ),
        (
            "an early return in a case arm with a final return",
            "fn f(txt s) { switch(s) { case(\"a\") { return \"a\"; }; }; return \"\"; };\nfn main() {};",
        ),
        (
            "unreachable code after a return",
            "fn f() { return \"a\"; return \"b\"; };\nfn main() {};",
        ),
        ("return in main is discarded", "fn main() { return \"\"; };"),
        (
            "a value function that panics on every other path",
            "fn f(txt s) { switch(s) { case(\"a\") { return \"a\"; }; default { panic; }; }; };\nfn main() {};",
        ),
    ]);
    expect_rejections(&[
        (
            "return inside a defer body",
            "fn f() { defer { return \"a\"; }; return \"\"; };\nfn main() {};",
            "`f` must not `return` inside `defer`",
        ),
        (
            "a value function that can fall through",
            "fn f(txt s) { switch(s) { case(\"a\") { return \"a\"; }; }; };\nfn main() {};",
            "`f` returns text but can fall through; every path must end in `return` or `panic`",
        ),
        (
            "returns of different kinds",
            "fn f(txt s) { switch(s) { case(\"a\") { return \"a\"; }; default { return { k = \"v\" }; }; }; };\nfn main() {};",
            "expected text, found record",
        ),
        (
            "returning a void call",
            "fn work() {};\nfn f() { return work(); };\nfn main() {};",
            "expected text or record, found nothing",
        ),
    ]);
}

#[test]
fn panic_and_void_rules() {
    expect_accepts(&[("panic as a statement", "fn main() { panic; };")]);
    expect_rejections(&[
        (
            "eprint bound to text",
            "fn main() { txt x = std::eprint(\"x\"); };",
            "expected text, found nothing",
        ),
        (
            "eprint passed as a text argument",
            "fn take(txt x) {};\nfn main() { take(std::eprint(\"x\")); };",
            "expected text, found nothing",
        ),
        (
            "eprint in a record field",
            "fn main() { rec r = { k = std::eprint(\"x\") }; };",
            "expected text, found nothing",
        ),
    ]);
}

#[test]
fn run_and_field_rules() {
    expect_accepts(&[
        (
            "run field returned through a function",
            "fn quiet(txt line) { return std::run(line).out; };\nfn main() { txt code = quiet(\"echo hi\"); };",
        ),
        (
            "run code field",
            "fn code(txt line) { return std::run(line).code; };\nfn main() {};",
        ),
        (
            "run at a module value",
            "txt x = std::run(\"echo hi\").out;\nfn main() {};",
        ),
        (
            "run field inside a record",
            "rec r = { out = std::run(\"echo hi\").out };\nfn main() {};",
        ),
        (
            "run as a bare statement",
            "fn main() { std::run(\"true\"); };",
        ),
    ]);
    expect_rejections(&[
        (
            "run bound to text",
            "fn main() { txt x = std::run(\"ls\"); };",
            "expected text, found record",
        ),
        (
            "run passed as a text argument",
            "fn take(txt x) {};\nfn main() { take(std::run(\"ls\")); };",
            "expected text, found record",
        ),
        (
            "run without an argument",
            "fn main() { std::run(); };",
            "`std::run` takes 1 arguments, found 0",
        ),
        (
            "run with a record argument",
            "fn main() { std::run({}); };",
            "expected text, found record",
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
            "fn read(rec repo) { return repo.dir; };\nfn main() { read({ dir = \"/x\" }); };",
        ),
        (
            "record parameter through a call",
            "rec backend = { dir = \"/b\" };\nfn read_it(rec repo) { return repo.dir; };\nfn main() { read_it(backend); };",
        ),
        (
            "text parameter through a call",
            "fn identity(txt value) { return value; };\nfn main() { std::print(identity(\"x\")); };",
        ),
        (
            "record parameter through a call with a literal",
            "fn identity(rec repo) { return repo.dir; };\nfn main() { std::print(identity({ dir = \"/x\" })); };",
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
            "fn read(rec repo) { return repo.dir; };\nfn main() { read(\"x\"); };",
            "expected record, found text",
        ),
    ]);
}

#[test]
fn async_wait_and_defer_rules() {
    expect_accepts(&[
        (
            "async and defer",
            "fn work() {};\nfn main() {\nstd::async(work());\nstd::wait();\ndefer { std::run(\"b\"); };\n};",
        ),
        (
            "void function as a statement and an async invocation",
            "fn work() {};\nfn main() {\nwork();\nstd::async(work());\nstd::wait();\n};",
        ),
        (
            "async of a native call",
            "fn main() { std::async(std::wait()); };",
        ),
        (
            "async of a call returning a record",
            "fn main() { std::async(std::run(\"true\")); };",
        ),
        (
            "bare async statement",
            "fn work() {};\nfn main() { std::async(work()); };",
        ),
        (
            "wait as a statement",
            "fn work() {};\nfn main() { std::async(work()); std::wait(); };",
        ),
    ]);
    expect_rejections(&[
        (
            "void function as a text binding",
            "fn work() {};\nfn main() { txt x = work(); };",
            "expected text, found nothing",
        ),
        (
            "void function as a text argument",
            "fn work() {};\nfn take(txt x) {};\nfn main() { take(work()); };",
            "expected text, found nothing",
        ),
        (
            "async of a non-call",
            "fn main() { std::async(\"work\"); };",
            "`std::async` takes a call",
        ),
        (
            "async at the module level",
            "fn work() {};\ntxt handle = std::async(work());\nfn main() {};",
            "`std::async` is only allowed inside a function",
        ),
        (
            "wait bound to a binding",
            "fn main() { txt x = std::wait(); };",
            "expected text, found nothing",
        ),
        (
            "wait as an argument",
            "fn take(txt x) {};\nfn main() { take(std::wait()); };",
            "expected text, found nothing",
        ),
        (
            "wait as a record field",
            "fn main() { rec r = { k = std::wait() }; };",
            "expected text, found nothing",
        ),
        (
            "wait with an argument",
            "fn main() { std::wait(\"x\"); };",
            "`std::wait` takes 0 arguments, found 1",
        ),
    ]);
}

#[test]
fn bindings_and_scope_rules() {
    expect_accepts(&[
        (
            "defer body declares into its enclosing body",
            "fn f() {\ntxt x = \"a\";\ndefer { txt y = \"b\"; x = y; };\nreturn x;\n};\nfn main() {};",
        ),
        (
            "case patterns of any text expression",
            "fn pick() { return \"a\"; };\nfn main() { switch(\"a\") { case(pick()) {}; }; };",
        ),
        (
            "case patterns that differ structurally",
            "fn pick() { return \"a\"; };\nfn other() { return \"a\"; };\nfn main() {\nswitch(\"a\") { case(pick()) {}; case(other()) {}; };\n};",
        ),
    ]);
    expect_rejections(&[
        (
            "parameter assignment",
            "fn f(txt x) { x = \"a\"; };\nfn main() {};",
            "`x` is a parameter and is read-only",
        ),
        (
            "module assignment",
            "txt x = \"a\";\nfn main() { x = \"b\"; };",
            "`x` is a module-level constant and cannot be assigned",
        ),
        (
            "assigning a record to a text local",
            "fn main() { txt x = \"a\"; x = {}; };",
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
            "expected text, found record",
        ),
        (
            "case arm does not leak locals",
            "fn main() {\nswitch(\"a\") {\ncase(\"a\") { txt hidden = \"x\"; };\ncase(\"b\") { txt seen = hidden; };\n};\n};",
            "unknown name `hidden`",
        ),
        (
            "defer declared name is not visible after the defer",
            "fn f() {\ndefer { txt hidden = \"b\"; };\nreturn hidden;\n};\nfn main() {};",
            "unknown name `hidden`",
        ),
        (
            "structurally duplicate call case patterns",
            "fn pick() { return \"a\"; };\nfn main() {\nswitch(\"a\") { case(pick()) {}; case(pick()) {}; };\n};",
            "duplicate case pattern",
        ),
        (
            "structurally duplicate reference case patterns",
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
            "txt value = \"root\";\nfn main() {\ntxt value = \"local\";\nstd::print(value);\nstd::print(::value);\n};",
        ),
        (
            "a value and a function share a name",
            "fn build() { return \"built\"; };\ntxt build = \"text\";\nfn main() { std::print(build); std::print(build()); };",
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
            "txt value = \"root\";\nfn main() { std::print(::missing); };",
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
        "fn work() {};\nfn main() {\nwork();\nstd::run(\"true\");\npanic;\n};",
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
fn checks_the_shipped_library_kinds() {
    let program = checked(
        "rec backend = { dir = \"/kiru\" };\n\
         fn read(rec repo) { return repo.dir; };\n\
         fn main() {\n\
           std::print(\"x\");\n\
           read(backend);\n\
         };",
    );

    let command = declaration(&program, &["std"], "command");
    assert_eq!(program.declaration(command).derived.kind, Some(Kind::Text));

    let print = declaration(&program, &["std"], "print");
    assert_eq!(
        program
            .declaration(first_parameter(&program, print))
            .derived
            .kind,
        Some(Kind::Text)
    );
    assert_eq!(program.declaration(print).derived.kind, Some(Kind::Nothing));

    let eprint = declaration(&program, &["std"], "eprint");
    assert_eq!(
        program.declaration(eprint).derived.kind,
        Some(Kind::Nothing)
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
