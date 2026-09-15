//! End to end tests for the parser grammar layers.

use super::parse_file;
use crate::syntax::ast::{Declaration, File, Statement, ValueKind};

fn parse(source: &str) -> File {
    parse_file(source).expect("source parses")
}

fn parse_error(source: &str) -> String {
    parse_file(source).expect_err("source is rejected").message
}

/// Run every rejection case against its exact expected message, collecting all
/// mismatches so one run reports every case that failed.
fn expect_rejections(cases: &[(&str, &str, &str)]) {
    let mut failures = Vec::new();
    for &(name, source, expected) in cases {
        let message = parse_error(source);
        if message != expected {
            failures.push(format!("{name}: expected `{expected}`, found `{message}`"));
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
fn parses_module_and_imports() {
    let file = parse("module tasks::build;\nimport \"std/repo.kiru\";\n");
    assert_eq!(
        file.module.expect("has a module").segments,
        vec!["tasks", "build"]
    );
    assert_eq!(file.imports[0].path, "std/repo.kiru");
}

#[test]
fn parses_function_with_switch_and_calls() {
    let file = parse(
        "fn main(rec args) {\n\
           switch(args.cmd) {\n\
             case(\"ci\") { \n\
               txt code = std::run(\"cargo test\");\n\
               txt done = \"done\" + code;\n\
               std::print(done);\n\
             };\n\
             default { std::print(\"\"); };\n\
           };\n\
         };",
    );
    let Declaration::Function(function) = &file.declarations[0] else {
        panic!("expected a function");
    };
    assert_eq!(function.name, "main");
    assert_eq!(function.parameters[0].name, "args");
    assert_eq!(function.parameters[0].kind, ValueKind::Record);
    assert_eq!(function.body.len(), 1);
}

#[test]
fn parses_typed_parameters() {
    let file = parse("fn f(txt a, rec b) -> txt { return(a); };");
    let Declaration::Function(function) = &file.declarations[0] else {
        panic!("expected a function");
    };
    assert_eq!(function.parameters[0].name, "a");
    assert_eq!(function.parameters[0].kind, ValueKind::Text);
    assert_eq!(function.parameters[1].name, "b");
    assert_eq!(function.parameters[1].kind, ValueKind::Record);
    assert_eq!(function.return_kind, Some(ValueKind::Text));
}

#[test]
fn parses_return_kinds() {
    let file = parse(
        "fn text_value() -> txt { return(\"\"); };\n\
         fn record_value() -> rec { return({}); };\n\
         fn no_value() { return(); };",
    );
    let Declaration::Function(text_value) = &file.declarations[0] else {
        panic!("expected a function");
    };
    assert_eq!(text_value.return_kind, Some(ValueKind::Text));
    let Declaration::Function(record_value) = &file.declarations[1] else {
        panic!("expected a function");
    };
    assert_eq!(record_value.return_kind, Some(ValueKind::Record));
    let Declaration::Function(no_value) = &file.declarations[2] else {
        panic!("expected a function");
    };
    assert_eq!(no_value.return_kind, None);
}

#[test]
fn rejects_a_parameter_without_a_kind() {
    let message = parse_error("fn f(a) { return(a); };");
    assert!(message.contains("expected `txt` or `rec`"), "{message}");
}

#[test]
fn rejects_a_return_arrow_without_a_kind() {
    let message = parse_error("fn f() -> { return(); };");
    assert_eq!(message, "expected `txt` or `rec` after `->`, found `{`");
}

#[test]
fn parses_async_call_and_defer() {
    let file = parse(
        "fn run() {\n\
           async other();\n\
           defer { std::run(\"clean\"); };\n\
           wait;\n\
         };",
    );
    let Declaration::Function(function) = &file.declarations[0] else {
        panic!("expected a function");
    };
    assert!(matches!(function.body[0], Statement::Async { .. }));
    assert!(matches!(function.body[1], Statement::Defer { .. }));
    assert!(matches!(function.body[2], Statement::Wait { .. }));
}

#[test]
fn rejects_nested_async() {
    let message = parse_error("fn main() { async async work(); };");
    assert!(message.contains("expected an expression"), "{message}");
}

#[test]
fn parses_a_rooted_path() {
    let file = parse("fn main() { std::print(::value); };");
    let Declaration::Function(function) = &file.declarations[0] else {
        panic!("expected a function");
    };
    let Statement::Expression(crate::syntax::Expression::Call { arguments, .. }) =
        &function.body[0]
    else {
        panic!("expected a call");
    };
    let crate::syntax::Expression::Name { root, path, .. } = &arguments[0] else {
        panic!("expected a name");
    };
    assert!(*root);
    assert_eq!(path, &vec!["value".to_owned()]);
}

#[test]
fn parses_value_void_and_panic_returns() {
    let file =
        parse("fn f(txt a) -> txt { return(a); };\nfn g() { return(); };\nfn h() { panic; };");
    let Declaration::Function(value) = &file.declarations[0] else {
        panic!("expected a function");
    };
    assert!(matches!(
        value.body[0],
        Statement::Return { value: Some(_), .. }
    ));
    let Declaration::Function(empty) = &file.declarations[1] else {
        panic!("expected a function");
    };
    assert!(matches!(
        empty.body[0],
        Statement::Return { value: None, .. }
    ));
    let Declaration::Function(panics) = &file.declarations[2] else {
        panic!("expected a function");
    };
    assert!(matches!(panics.body[0], Statement::Panic { .. }));
}

#[test]
fn rejects_missing_semicolon() {
    let message = parse_error("txt x = \"a\"\n");
    assert!(message.contains("after the initializer"), "{message}");
}

#[test]
fn parses_field_assignment() {
    let file = parse("fn f(rec args) { args.cmd = \"x\"; };");
    let Declaration::Function(function) = &file.declarations[0] else {
        panic!("expected a function");
    };
    assert!(matches!(
        &function.body[0],
        Statement::FieldAssignment { field, .. } if field == "cmd"
    ));
}

#[test]
fn parses_mut_bindings_and_parameters() {
    let file = parse("fn f(mut txt x) { mut rec r = { a = \"1\" }; r.a = \"2\"; };");
    let Declaration::Function(function) = &file.declarations[0] else {
        panic!("expected a function");
    };
    assert!(function.parameters[0].mutable);
    let Statement::Binding(binding) = &function.body[0] else {
        panic!("expected a binding");
    };
    assert!(binding.mutable);
}

#[test]
fn rejects_mut_module_value() {
    let message = parse_error("mut txt x = \"a\";");
    assert_eq!(message, "a module value cannot be `mut`");
}

#[test]
fn parser_errors() {
    expect_rejections(&[
        (
            "module after a declaration",
            "fn a() {};\nmodule b;",
            "`module` must be the first declaration",
        ),
        (
            "import after a declaration",
            "txt x = \"a\";\nimport \"b.kiru\";\n",
            "imports must come before declarations",
        ),
        (
            "namespaced declaration",
            "foo::bar rec x = { dir = \"b\" };\n",
            "expected a declaration, found `foo`",
        ),
    ]);
}
