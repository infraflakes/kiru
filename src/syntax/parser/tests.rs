//! End to end tests for the parser grammar layers.

use super::parse_file;
use crate::syntax::ast::{Declaration, Expression, File, Item, Statement};
use crate::types::Type;

fn parse(source: &str) -> File {
    parse_file(source).expect("source parses")
}

fn parse_error(source: &str) -> String {
    parse_file(source).expect_err("source is rejected").message
}

/// The declaration at a top-level item, for the cases that write one.
fn declaration(file: &File, index: usize) -> &Declaration {
    match &file.items[index] {
        Item::Declaration(declaration) => declaration,
        other => panic!("expected a declaration, found {other:?}"),
    }
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
fn parses_a_mod_block() {
    let file = parse("mod a::b { fn f() {}; };");
    let Item::Module(module) = &file.items[0] else {
        panic!("expected a module");
    };
    assert_eq!(module.path, vec!["a".to_owned(), "b".to_owned()]);
    assert_eq!(module.declarations.len(), 1);
}

#[test]
fn parses_mod_and_import() {
    let file = parse("import \"std/repo.kiru\";\nmod tasks::build { fn f() {}; };");
    let Item::Import(import) = &file.items[0] else {
        panic!("expected an import");
    };
    assert_eq!(import.path, "std/repo.kiru");
    let Item::Module(module) = &file.items[1] else {
        panic!("expected a module");
    };
    assert_eq!(module.path, vec!["tasks".to_owned(), "build".to_owned()]);
}

#[test]
fn parses_function_with_match_and_calls() {
    let file = parse(
        "fn main(args<rec>) {\n\
           match args.cmd {\n\
             \"ci\" => {\n\
               let code<txt> = std::command({}, \"cargo test\");\n\
               let done<txt> = \"done\" + code;\n\
               std::print(done);\n\
             };\n\
             _ => { std::print(\"\"); };\n\
           };\n\
         };",
    );
    let Declaration::Function(function) = declaration(&file, 0) else {
        panic!("expected a function");
    };
    assert_eq!(function.name, "main");
    assert_eq!(function.parameters[0].name, "args");
    assert_eq!(function.parameters[0].ty, Type::Record);
    assert_eq!(function.body.len(), 1);
    assert!(matches!(
        function.body[0],
        Statement::Expression(Expression::Match { .. })
    ));
}

#[test]
fn parses_typed_parameters() {
    let file = parse("fn f(a<txt>, b<rec>) -> txt { return a; };");
    let Declaration::Function(function) = declaration(&file, 0) else {
        panic!("expected a function");
    };
    assert_eq!(function.parameters[0].name, "a");
    assert_eq!(function.parameters[0].ty, Type::Text);
    assert_eq!(function.parameters[1].name, "b");
    assert_eq!(function.parameters[1].ty, Type::Record);
    assert_eq!(function.return_type, Some(Type::Text));
}

#[test]
fn parses_return_types() {
    let file = parse(
        "fn text_value() -> txt { return \"\"; };\n\
         fn record_value() -> rec { return {}; };\n\
         fn no_value() { return; };",
    );
    let Declaration::Function(text_value) = declaration(&file, 0) else {
        panic!("expected a function");
    };
    assert_eq!(text_value.return_type, Some(Type::Text));
    let Declaration::Function(record_value) = declaration(&file, 1) else {
        panic!("expected a function");
    };
    assert_eq!(record_value.return_type, Some(Type::Record));
    let Declaration::Function(no_value) = declaration(&file, 2) else {
        panic!("expected a function");
    };
    assert_eq!(no_value.return_type, None);
}

#[test]
fn rejects_a_parameter_without_a_type() {
    let message = parse_error("fn f(a) { return a; };");
    assert!(
        message.contains("expected `<` in the parameter list"),
        "{message}"
    );
}

#[test]
fn rejects_a_return_arrow_without_a_type() {
    let message = parse_error("fn f() -> { return; };");
    assert_eq!(
        message,
        "expected `txt`, `rec` or `list` after `->`, found `{`"
    );
}

#[test]
fn parses_async_call_and_wait() {
    let file = parse(
        "fn run() {\n\
           async other();\n\
           wait;\n\
         };",
    );
    let Declaration::Function(function) = declaration(&file, 0) else {
        panic!("expected a function");
    };
    assert!(matches!(function.body[0], Statement::Async { .. }));
    assert!(matches!(function.body[1], Statement::Wait { .. }));
}

#[test]
fn rejects_nested_async() {
    let message = parse_error("fn main() { async async work(); };");
    assert!(message.contains("expected an expression"), "{message}");
}

#[test]
fn parses_a_rooted_path() {
    let file = parse("fn main() { std::print(::value); };");
    let Declaration::Function(function) = declaration(&file, 0) else {
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
    let file = parse("fn f(a<txt>) -> txt { return a; };\nfn g() { return; };\nfn h() { panic; };");
    let Declaration::Function(value) = declaration(&file, 0) else {
        panic!("expected a function");
    };
    assert!(matches!(
        value.body[0],
        Statement::Return { value: Some(_), .. }
    ));
    let Declaration::Function(empty) = declaration(&file, 1) else {
        panic!("expected a function");
    };
    assert!(matches!(
        empty.body[0],
        Statement::Return { value: None, .. }
    ));
    let Declaration::Function(panics) = declaration(&file, 2) else {
        panic!("expected a function");
    };
    assert!(matches!(panics.body[0], Statement::Panic { .. }));
}

#[test]
fn rejects_missing_semicolon() {
    let message = parse_error("let x<txt> = \"a\"\n");
    assert!(message.contains("after the initializer"), "{message}");
}

#[test]
fn parses_field_assignment() {
    let file = parse("fn f(args<rec>) { args.cmd = \"x\"; };");
    let Declaration::Function(function) = declaration(&file, 0) else {
        panic!("expected a function");
    };
    assert!(matches!(
        &function.body[0],
        Statement::FieldAssignment { field, .. } if field == "cmd"
    ));
}

#[test]
fn parses_mut_bindings_and_parameters() {
    let file = parse("fn f(mut x<txt>) { let mut r<rec> = { a = \"1\" }; r.a = \"2\"; };");
    let Declaration::Function(function) = declaration(&file, 0) else {
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
    let message = parse_error("let mut x<txt> = \"a\";");
    assert_eq!(message, "a module value cannot be `mut`");
}

#[test]
fn parser_errors() {
    expect_rejections(&[
        (
            "namespaced declaration",
            "foo::bar let x<rec> = { dir = \"b\" };\n",
            "expected a declaration, found `foo`",
        ),
        (
            "import inside a mod",
            "mod a { import \"b.kiru\"; };",
            "expected a declaration, found `import`",
        ),
    ]);
}
