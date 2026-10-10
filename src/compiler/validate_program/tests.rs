use crate::compiler::{checked_files, checked_program};
use crate::model::{DeclarationId, DeclarationKind, Program};
use crate::native_registry::native_row;
use crate::types::Type;

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

/// The type a node declares: a function's return type, a value or binding's
/// type, or a native's row result.
fn value_type(program: &Program, id: DeclarationId) -> Type {
    match &program.declaration(id).kind {
        DeclarationKind::Function(function) => function.return_type,
        DeclarationKind::Value { ty, .. } => *ty,
        DeclarationKind::Binding(binding) => binding.ty,
        DeclarationKind::Native(native) => native_row(*native).returns,
    }
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
        ("main with a record parameter", "fn main(args<rec>) {};"),
        (
            "function without a return",
            "fn f() { let x<txt> = \"a\"; };\nfn main() {};",
        ),
        (
            "function with a return",
            "fn f() -> txt { return \"a\"; };\nfn main() {};",
        ),
        (
            "an early return in a match arm with a final return",
            "fn f(s<txt>) -> txt { match s { \"a\" => { return \"a\"; }; }; return \"\"; };\nfn main() {};",
        ),
        (
            "unreachable code after a return",
            "fn f() -> txt { return \"a\"; return \"b\"; };\nfn main() {};",
        ),
        ("a bare return in main", "fn main() { return; };"),
        (
            "a value-returning main",
            "fn main() -> txt { return \"\"; };",
        ),
        (
            "a value function that panics on every other path",
            "fn f(s<txt>) -> txt { match s { \"a\" => { return \"a\"; }; _ => { panic; }; }; };\nfn main() {};",
        ),
        (
            "a value function that panics on every path",
            "fn f() -> txt { panic; };\nfn main() {};",
        ),
        (
            "a value function that ends in a halting call",
            "fn f() -> txt { std::eprint(\"x\"); };\nfn main() {};",
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
            "fn f(s<txt>) -> txt { match s { \"a\" => { return \"a\"; }; }; };\nfn main() {};",
            "`f` is declared to return text, so every path must end with `return ...`, `panic;`, or a call that stops the run",
        ),
        (
            "a returned value of the wrong type",
            "fn f(s<txt>) -> txt { match s { \"a\" => { return \"a\"; }; _ => { return { k = \"v\" }; }; }; };\nfn main() {};",
            "expected text, found record",
        ),
        (
            "returning a nothing call",
            "fn work() {};\nfn f() -> txt { return work(); };\nfn main() {};",
            "expected text, found void",
        ),
        (
            "a value return in a nothing function",
            "fn f() { return \"a\"; };\nfn main() {};",
            "`f` is declared to return nothing, so `return` cannot carry a value",
        ),
        (
            "a bare return in a value function",
            "fn f() -> txt { return; };\nfn main() {};",
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
            "fn main() { let x<txt> = std::eprint(\"x\"); };",
            "expected text, found void",
        ),
        (
            "eprint passed as a text argument",
            "fn take(x<txt>) {};\nfn main() { take(std::eprint(\"x\")); };",
            "expected text, found void",
        ),
        (
            "eprint in a record field",
            "fn main() { let r<rec> = { k = std::eprint(\"x\") }; };",
            "expected text, found void",
        ),
    ]);
}

#[test]
fn command_and_field_rules() {
    expect_accepts(&[
        (
            "command code returned through a function",
            "fn code(line<txt>) -> txt { return std::command({}, line); };\nfn main() { let code<txt> = code(\"echo hi\"); };",
        ),
        (
            "command at a module value",
            "let x<txt> = std::command({}, \"echo hi\");\nfn main() {};",
        ),
        (
            "command inside a record",
            "let r<rec> = { code = std::command({}, \"echo hi\") };\nfn main() {};",
        ),
        (
            "command as a bare statement",
            "fn main() { std::command({}, \"true\"); };",
        ),
    ]);
    expect_rejections(&[
        (
            "command bound to a record",
            "fn main() { let x<rec> = std::command({}, \"ls\"); };",
            "expected record, found text",
        ),
        (
            "command passed as a record argument",
            "fn take(x<rec>) {};\nfn main() { take(std::command({}, \"ls\")); };",
            "expected record, found text",
        ),
        (
            "command without arguments",
            "fn main() { std::command(); };",
            "`std::command` takes 2 arguments, found 0",
        ),
        (
            "command with one argument",
            "fn main() { std::command({}); };",
            "`std::command` takes 2 arguments, found 1",
        ),
        (
            "command with a text spec",
            "fn main() { std::command(\"ls\", \"\"); };",
            "expected record, found text",
        ),
        (
            "field access on text",
            "fn main() { let x<txt> = \"a\".b; };",
            "expected record, found text",
        ),
    ]);
}

#[test]
fn text_and_record_rules() {
    expect_accepts(&[
        (
            "record argument",
            "fn read(repo<rec>) -> txt { return repo.dir; };\nfn main() { read({ dir = \"/x\" }); };",
        ),
        (
            "record parameter through a call",
            "let backend<rec> = { dir = \"/b\" };\nfn read_it(repo<rec>) -> txt { return repo.dir; };\nfn main() { read_it(backend); };",
        ),
        (
            "text parameter through a call",
            "fn identity(value<txt>) -> txt { return value; };\nfn main() { std::print(identity(\"x\")); };",
        ),
        (
            "record parameter through a call with a literal",
            "fn identity(repo<rec>) -> txt { return repo.dir; };\nfn main() { std::print(identity({ dir = \"/x\" })); };",
        ),
    ]);
    expect_rejections(&[
        (
            "plus on a record",
            "fn main() { let x<txt> = \"a\" + { b = \"c\" }; };",
            "expected text, found record",
        ),
        (
            "text parameter used as a record",
            "fn f(x<txt>) { let a<txt> = x; let b<txt> = x.b; };\nfn main() { f(\"s\"); };",
            "expected record, found text",
        ),
        (
            "text where a record is required",
            "fn read(repo<rec>) -> txt { return repo.dir; };\nfn main() { read(\"x\"); };",
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
            "fn main() { async std::command({}, \"true\"); };",
        ),
        (
            "async of a call returning text",
            "fn pick() -> txt { return \"x\"; };\nfn main() { async pick(); wait; };",
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
            "let x<txt> = \"a\";\nfn main() { async x; };",
            "`async` takes a call",
        ),
    ]);
}

#[test]
fn bindings_and_scope_rules() {
    expect_accepts(&[(
        "literal, name, and field-path match arms",
        "let spec<rec> = { cmd = \"a\" };\nfn main() {\nlet a<txt> = \"a\";\nmatch \"a\" { \"a\" => {}; a => {}; spec.cmd => {}; };\n};",
    )]);
    expect_rejections(&[
        (
            "parameter assignment",
            "fn f(x<txt>) { x = \"a\"; };\nfn main() {};",
            "`x` is not mutable; declare it `mut`",
        ),
        (
            "module assignment",
            "let x<txt> = \"a\";\nfn main() { x = \"b\"; };",
            "`x` is a module-level constant and cannot be assigned",
        ),
        (
            "assigning a record to a text local",
            "fn main() { let mut x<txt> = \"a\"; x = {}; };",
            "expected text, found record",
        ),
        (
            "duplicate literal match pattern",
            "fn main() { match \"a\" { \"a\" => {}; \"a\" => {}; }; };",
            "duplicate match pattern `a`",
        ),
        (
            "record match pattern",
            "fn main() { match \"a\" { {} => {}; }; };",
            "a match arm is a literal, a name, or a field path",
        ),
        (
            "match arm does not leak locals",
            "fn main() {\nmatch \"a\" {\n\"a\" => { let hidden<txt> = \"x\"; };\n\"b\" => { let seen<txt> = hidden; };\n};\n};",
            "unknown name `hidden`",
        ),
        (
            "a call match arm",
            "fn pick() -> txt { return \"a\"; };\nfn main() {\nmatch \"a\" { pick() => {}; };\n};",
            "a match arm is a literal, a name, or a field path",
        ),
        (
            "a concatenation match arm",
            "fn main() {\nmatch \"ab\" { \"a\" + \"b\" => {}; };\n};",
            "a match arm is a literal, a name, or a field path",
        ),
        (
            "duplicate reference match patterns",
            "fn main() {\nlet a<txt> = \"a\";\nmatch \"a\" { a => {}; a => {}; };\n};",
            "duplicate match pattern",
        ),
    ]);
}

#[test]
fn match_expression_rules() {
    expect_accepts(&[
        (
            "an expression match yields a value",
            "fn f() -> txt { return match \"a\" { \"a\" => \"one\"; _ => \"other\"; }; };\nfn main() {};",
        ),
        (
            "a record-valued match",
            "fn f() -> txt { let r<rec> = match \"a\" { \"a\" => { k = \"v\" }; _ => { k = \"w\" }; }; return r.k; };\nfn main() {};",
        ),
    ]);
    expect_rejections(&[
        (
            "match arms of different types",
            "fn main() { let x<txt> = match \"a\" { \"a\" => \"one\"; _ => { k = \"v\" }; }; };",
            "expected text, found record",
        ),
        (
            "a text match assigned to a record",
            "fn main() { let x<rec> = match \"a\" { \"a\" => \"one\"; _ => \"two\"; }; };",
            "expected record, found text",
        ),
    ]);
}

#[test]
fn name_and_call_rules() {
    expect_accepts(&[
        (
            "root access when a local shadows the root",
            "let value<txt> = \"root\";\nfn main() {\nlet value<txt> = \"local\";\nstd::print(value);\nstd::print(::value);\n};",
        ),
        (
            "a value and a function share a name",
            "fn build() -> txt { return \"built\"; };\nlet build<txt> = \"text\";\nfn main() { std::print(build); std::print(build()); };",
        ),
    ]);
    expect_rejections(&[
        (
            "function as a value",
            "fn f() {};\nfn main() { let x<txt> = f; };",
            "`f` is a function; call it",
        ),
        (
            "calling a module value",
            "let x<txt> = \"a\";\nfn main() { x(); };",
            "`x` is a value and cannot be called",
        ),
        (
            "calling a local",
            "fn main() { let x<txt> = \"a\"; x(); };",
            "unknown name `x`",
        ),
        (
            "wrong arity",
            "fn f(a<txt>) {};\nfn main() { f(); };",
            "`f` takes 1 arguments, found 0",
        ),
        (
            "unknown rooted name",
            "let value<txt> = \"root\";\nfn main() { std::print(::missing); };",
            "unknown name `::missing`",
        ),
        (
            "function self reference",
            "fn f() { f(); };\nfn main() {};",
            "a function cannot reference itself",
        ),
        (
            "value self reference",
            "let x<txt> = x;\nfn main() {};",
            "a value cannot reference itself",
        ),
    ]);
}

#[test]
fn statement_form_rules() {
    expect_accepts(&[(
        "bare call and panic statements",
        "fn work() {};\nfn main() {\nwork();\nstd::command({}, \"true\");\npanic;\n};",
    )]);
    expect_rejections(&[
        (
            "bare literal statement",
            "fn main() { \"x\"; };",
            "a statement must be a call",
        ),
        (
            "bare reference statement",
            "fn main() { let name<txt> = \"x\"; name; };",
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
            "fn f() { let mut x<txt> = \"a\"; x = \"b\"; };\nfn main() {};",
        ),
        (
            "field assign a mut record",
            "fn f() { let mut r<rec> = { a = \"1\" }; r.a = \"2\"; };\nfn main() {};",
        ),
        (
            "field assign adds a field",
            "fn f() { let mut r<rec> = { a = \"1\" }; r.b = \"2\"; };\nfn main() {};",
        ),
        (
            "rebind a mut parameter",
            "fn f(mut x<txt>) { x = \"b\"; };\nfn main() {};",
        ),
        (
            "field assign a mut parameter",
            "fn f(mut r<rec>) { r.a = \"b\"; };\nfn main() {};",
        ),
    ]);
    expect_rejections(&[
        (
            "rebind an immutable local",
            "fn f() { let x<txt> = \"a\"; x = \"b\"; };\nfn main() {};",
            "`x` is not mutable; declare it `mut`",
        ),
        (
            "field assign an immutable record",
            "fn f() { let r<rec> = { a = \"1\" }; r.a = \"2\"; };\nfn main() {};",
            "`r` is not mutable; declare it `mut`",
        ),
        (
            "field assign an immutable parameter",
            "fn f(x<txt>) { x.a = \"b\"; };\nfn main() {};",
            "`x` is not mutable; declare it `mut`",
        ),
        (
            "field assign a text binding",
            "fn f() { let mut x<txt> = \"a\"; x.b = \"c\"; };\nfn main() {};",
            "`x` is not a record and has no fields",
        ),
        (
            "field assign a module value",
            "let r<rec> = { a = \"1\" };\nfn main() { r.a = \"2\"; };",
            "`r` is a module-level constant and cannot be assigned",
        ),
    ]);
}

#[test]
fn checks_the_shipped_library_types() {
    let program = checked_program(
        "let backend<rec> = { dir = \"/kiru\" };\n\
         fn read(repo<rec>) -> txt { return repo.dir; };\n\
         fn main() {\n\
           std::print(\"x\");\n\
           read(backend);\n\
         };",
    );

    let command = declaration(&program, &["std"], "command");
    assert_eq!(value_type(&program, command), Type::Text);

    let print = declaration(&program, &["std"], "print");
    assert_eq!(
        value_type(&program, first_parameter(&program, print)),
        Type::Text
    );
    assert_eq!(value_type(&program, print), Type::Void);

    let eprint = declaration(&program, &["std"], "eprint");
    assert_eq!(value_type(&program, eprint), Type::Void);
    assert!(
        program.declaration(eprint).derived.halts,
        "eprint ends in `panic;`, so it stops the run"
    );

    let read = declaration(&program, &[], "read");
    assert_eq!(
        value_type(&program, first_parameter(&program, read)),
        Type::Record
    );
}

#[test]
fn list_and_loop_rules() {
    expect_accepts(&[
        (
            "a list literal binding and a for loop",
            "fn main() { let xs<list> = [\"a\", \"b\"]; for x in xs { std::print(x); }; };",
        ),
        (
            "a function that takes and returns a list",
            "fn id(xs<list>) -> list { return xs; }; fn main() { id([\"a\"]); };",
        ),
        (
            "a module-level list value",
            "let xs<list> = [\"a\"]; fn main() { for x in xs { std::print(x); }; };",
        ),
        (
            "break inside a for",
            "fn main() { for x in [\"a\"] { break; }; };",
        ),
        (
            "a for that skips an element with a match default",
            "fn main() { for x in [\"a\"] { match x { \"a\" => {}; _ => { std::print(x); }; }; }; };",
        ),
        (
            "an unbounded for that breaks",
            "fn main() { for { break; }; };",
        ),
        (
            "an unbounded for as the only path of a value function",
            "fn f() -> txt { for { std::print(\"a\"); }; }; fn main() {};",
        ),
    ]);
    expect_rejections(&[
        (
            "for over text",
            "fn main() { for x in \"a\" { std::print(x); }; };",
            "expected list, found text",
        ),
        (
            "break outside a loop",
            "fn main() { break; };",
            "`break` is only allowed inside a loop",
        ),
        (
            "a list element that is not text",
            "fn main() { let xs<list> = [{ k = \"v\" }]; };",
            "expected text, found record",
        ),
        (
            "a list binding whose value is not a list",
            "fn main() { let xs<list> = \"a\"; };",
            "expected list, found text",
        ),
        (
            "a value function whose only path is a loop",
            "fn f() -> txt { for x in [\"a\"] { return x; }; }; fn main() {};",
            "`f` is declared to return text, so every path must end with `return ...`, `panic;`, or a call that stops the run",
        ),
    ]);
}

#[test]
fn main_returns_text_or_nothing() {
    expect_accepts(&[
        ("main returning text", "fn main() -> txt { return \"0\"; };"),
        ("main returning nothing", "fn main() {};"),
    ]);
    expect_rejections(&[
        (
            "main returning a record",
            "fn main() -> rec { return {}; };",
            "`main` returns text or nothing",
        ),
        (
            "main returning a list",
            "fn main() -> list { return []; };",
            "`main` returns text or nothing",
        ),
    ]);
}
