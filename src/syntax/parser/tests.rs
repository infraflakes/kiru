//! End to end tests for the parser grammar layers.

use super::parse_file;
use crate::syntax::ast::{
    Declaration, Expression, File, Item, Statement, TypeDeclarationKind, TypeExpr,
};

fn parse(source: &str) -> File {
    parse_file(source).expect("source parses")
}

/// The name a written type names: a bare, unapplied single-segment path. A
/// built-in and a user type parse the same way.
fn written_name(expr: &TypeExpr) -> &str {
    match expr {
        TypeExpr::Named {
            root: false,
            path,
            arguments,
            ..
        } if arguments.is_empty() && path.len() == 1 => &path[0],
        other => panic!("expected a bare named type, found {other:?}"),
    }
}

/// The element type written inside a `Vec<...>` type, asserting the type is
/// spelled `Vec`.
fn vec_element(ty: &TypeExpr) -> &TypeExpr {
    let TypeExpr::Named {
        root: false,
        path,
        arguments,
        ..
    } = ty
    else {
        panic!("expected a named type, found {ty:?}");
    };
    assert_eq!(path, &["Vec".to_owned()]);
    assert_eq!(arguments.len(), 1, "a Vec type names its element type");
    &arguments[0]
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
fn parses_index_assignment() {
    let file = parse("fn main() { let xs: Vec<int> = []; xs[0] = 1; };");
    let Declaration::Function(function) = declaration(&file, 0) else {
        panic!("expected a function");
    };
    assert!(matches!(
        function.body[1],
        Statement::IndexAssignment { .. }
    ));
}

#[test]
fn parses_index_expressions() {
    let file = parse("fn f(xs: Vec<int>) -> int { return xs[0]; };");
    let Declaration::Function(function) = declaration(&file, 0) else {
        panic!("expected a function");
    };
    let Some(Statement::Return {
        value: Some(value), ..
    }) = function.body.first()
    else {
        panic!("expected a return");
    };
    let Expression::Index { target, index, .. } = value else {
        panic!("expected an index expression");
    };
    assert!(matches!(target.as_ref(), Expression::Name { .. }));
    assert!(matches!(index.as_ref(), Expression::Int { value: 0, .. }));
}

#[test]
fn parses_parameterized_vec_types() {
    let file = parse("fn f(xs: Vec<int>, nested: Vec<Vec<txt>>) -> Vec<txt> { return xs; };");
    let Declaration::Function(function) = declaration(&file, 0) else {
        panic!("expected a function");
    };
    assert_eq!(written_name(vec_element(&function.parameters[0].ty)), "int");

    let nested = vec_element(&function.parameters[1].ty);
    assert_eq!(written_name(vec_element(nested)), "txt");

    let Some(return_type) = &function.return_type else {
        panic!("expected a Vec return type");
    };
    assert_eq!(written_name(vec_element(return_type)), "txt");

    // A bare `Vec` parses as a name with no arguments; the resolver rejects it
    // for arity, because the element type is always written.
    let file = parse("fn f(xs: Vec) {};");
    let Declaration::Function(function) = declaration(&file, 0) else {
        panic!("expected a function");
    };
    let TypeExpr::Named {
        path, arguments, ..
    } = &function.parameters[0].ty
    else {
        panic!("expected a named type");
    };
    assert_eq!(path, &["Vec".to_owned()]);
    assert!(arguments.is_empty());
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
        "fn main(args: txt) {\n\
           match args.cmd {\n\
             \"ci\" => {\n\
               let code: txt = std::run({}, \"cargo test\");\n\
               let done: txt = \"done\" + code;\n\
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
    assert_eq!(written_name(&function.parameters[0].ty), "txt");
    assert_eq!(function.body.len(), 1);
    assert!(matches!(
        function.body[0],
        Statement::Expression(Expression::Match { .. })
    ));
}

#[test]
fn parses_sums_left_associatively() {
    let file = parse("fn f() -> txt { return \"a\" + \"b\" + \"c\"; };");
    let Declaration::Function(function) = declaration(&file, 0) else {
        panic!("expected a function");
    };
    let Statement::Return {
        value: Some(expression),
        ..
    } = &function.body[0]
    else {
        panic!("expected a return");
    };
    let Expression::Binary { left, right, .. } = expression else {
        panic!("expected a sum");
    };
    // `("a" + "b") + "c"`
    assert!(matches!(**left, Expression::Binary { .. }));
    assert!(matches!(**right, Expression::Text { .. }));
}

#[test]
fn parses_typed_parameters() {
    let file = parse("fn f(a: txt, b: bool) -> txt { return a; };");
    let Declaration::Function(function) = declaration(&file, 0) else {
        panic!("expected a function");
    };
    assert_eq!(function.parameters[0].name, "a");
    assert_eq!(written_name(&function.parameters[0].ty), "txt");
    assert_eq!(function.parameters[1].name, "b");
    assert_eq!(written_name(&function.parameters[1].ty), "bool");
    assert_eq!(function.return_type.as_ref().map(written_name), Some("txt"));
}

#[test]
fn parses_return_types() {
    let file = parse(
        "fn text_value() -> txt { return \"\"; };\n\
         fn bool_value() -> bool { return true; };\n\
         fn no_value() { return; };",
    );
    let Declaration::Function(text_value) = declaration(&file, 0) else {
        panic!("expected a function");
    };
    assert_eq!(
        text_value.return_type.as_ref().map(written_name),
        Some("txt")
    );
    let Declaration::Function(bool_value) = declaration(&file, 1) else {
        panic!("expected a function");
    };
    assert_eq!(
        bool_value.return_type.as_ref().map(written_name),
        Some("bool")
    );
    let Declaration::Function(no_value) = declaration(&file, 2) else {
        panic!("expected a function");
    };
    assert_eq!(no_value.return_type, None);
}

#[test]
fn parses_generic_type_parameters() {
    let file = parse("rec Pair<A, B> { first: A, second: B };");
    let Declaration::Type(type_decl) = declaration(&file, 0) else {
        panic!("expected a type declaration");
    };
    assert_eq!(type_decl.parameters, vec!["A".to_owned(), "B".to_owned()]);
    let TypeDeclarationKind::Rec(fields) = &type_decl.kind else {
        panic!("expected a `rec` declaration");
    };
    let TypeExpr::Named {
        path, arguments, ..
    } = &fields[0].ty
    else {
        panic!("expected a named type; bare `A` parses as a path");
    };
    assert_eq!(path, &["A".to_owned()]);
    assert!(arguments.is_empty());

    let file = parse("fn f(p: Pair<int, txt>) {};");
    let Declaration::Function(function) = declaration(&file, 0) else {
        panic!("expected a function");
    };
    let TypeExpr::Named { arguments, .. } = &function.parameters[0].ty else {
        panic!("expected a named type");
    };
    assert_eq!(arguments.len(), 2);
    assert_eq!(written_name(&arguments[0]), "int");
    assert_eq!(written_name(&arguments[1]), "txt");
}

#[test]
fn parses_generic_function_parameters() {
    let file = parse("fn pair<A, B>(a: A, b: B) -> A { return a; };");
    let Declaration::Function(function) = declaration(&file, 0) else {
        panic!("expected a function");
    };
    assert_eq!(
        function.type_parameters,
        vec!["A".to_owned(), "B".to_owned()]
    );
}

#[test]
fn parses_a_function_type() {
    let file = parse("fn apply(f: fn(int) -> int, x: int) -> int { return x; };");
    let Declaration::Function(function) = declaration(&file, 0) else {
        panic!("expected a function");
    };
    let TypeExpr::Function { parameters, .. } = &function.parameters[0].ty else {
        panic!("expected a function type");
    };
    assert_eq!(written_name(&parameters[0]), "int");
}

#[test]
fn parses_a_type_declaration() {
    let file = parse("rec Project { name: txt, count: int };");
    let Declaration::Type(declaration) = declaration(&file, 0) else {
        panic!("expected a type declaration");
    };
    assert_eq!(declaration.name, "Project");
    let TypeDeclarationKind::Rec(fields) = &declaration.kind else {
        panic!("expected a `rec` declaration");
    };
    assert_eq!(fields.len(), 2);
    assert_eq!(fields[0].name, "name");
    assert_eq!(written_name(&fields[0].ty), "txt");
    assert_eq!(fields[1].name, "count");
    assert_eq!(written_name(&fields[1].ty), "int");
}

#[test]
fn parses_a_builtin_type_declaration() {
    let file = parse("type txt;\ntype Vec<T>;\ntype pair<K, V>;");
    for (index, (name, parameters)) in [("txt", 0), ("Vec", 1), ("pair", 2)].iter().enumerate() {
        let Declaration::Type(declaration) = declaration(&file, index) else {
            panic!("expected a type declaration");
        };
        assert_eq!(&declaration.name, name);
        assert_eq!(declaration.parameters.len(), *parameters);
        assert_eq!(declaration.kind, TypeDeclarationKind::Builtin);
    }
}

#[test]
fn parses_an_enum_declaration() {
    let file = parse("enum Shape { Dot, Circle(int), Rect(int, int) };");
    let Declaration::Type(declaration) = declaration(&file, 0) else {
        panic!("expected a type declaration");
    };
    assert_eq!(declaration.name, "Shape");
    let TypeDeclarationKind::Enum(variants) = &declaration.kind else {
        panic!("expected an enum declaration");
    };
    assert_eq!(variants.len(), 3);
    assert_eq!(variants[0].name, "Dot");
    assert!(variants[0].payload.is_empty());
    assert_eq!(variants[1].name, "Circle");
    assert_eq!(written_name(&variants[1].payload[0]), "int");
    assert_eq!(variants[2].name, "Rect");
    assert_eq!(variants[2].payload.len(), 2);
}

#[test]
fn parses_a_record_literal() {
    let file = parse("fn main() { let p: Project = { name = \"x\" }; };");
    let Declaration::Function(function) = declaration(&file, 0) else {
        panic!("expected a function");
    };
    let Statement::Binding(binding) = &function.body[0] else {
        panic!("expected a binding");
    };
    let Expression::Map { fields, .. } = &binding.value else {
        panic!("expected a record literal");
    };
    assert_eq!(fields.len(), 1);
    assert_eq!(fields[0].name, "name");
}

#[test]
fn rejects_a_parameter_without_a_type() {
    let message = parse_error("fn f(a) { return a; };");
    assert!(
        message.contains("expected `:` in the parameter list"),
        "{message}"
    );
}

#[test]
fn rejects_a_return_arrow_without_a_type() {
    let message = parse_error("fn f() -> { return; };");
    assert_eq!(message, "expected a type after `->`, found `{`");
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
fn parses_value_no_value_and_panic_returns() {
    let file = parse("fn f(a: txt) -> txt { return a; };\nfn g() { return; };\nfn h() { panic; };");
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
    let message = parse_error("let x: txt = \"a\"\n");
    assert!(message.contains("after the initializer"), "{message}");
}

#[test]
fn parses_field_assignment() {
    let file = parse("fn f(args: P) { args.cmd = \"x\"; };");
    let Declaration::Function(function) = declaration(&file, 0) else {
        panic!("expected a function");
    };
    assert!(matches!(
        &function.body[0],
        Statement::FieldAssignment { field, .. } if field == "cmd"
    ));
}

#[test]
fn parses_bindings_and_field_assignment() {
    let file = parse("fn f(x: txt) { let r: P = { a = \"1\" }; r.a = \"2\"; };");
    let Declaration::Function(function) = declaration(&file, 0) else {
        panic!("expected a function");
    };
    assert!(matches!(function.body[0], Statement::Binding(_)));
    assert!(matches!(
        function.body[1],
        Statement::FieldAssignment { .. }
    ));
}

#[test]
fn parser_errors() {
    expect_rejections(&[
        (
            "namespaced declaration",
            "foo::bar let x: P = { dir = \"b\" };\n",
            "expected a declaration, found `foo`",
        ),
        (
            "import inside a mod",
            "mod a { import \"b.kiru\"; };",
            "expected a declaration, found `import`",
        ),
        (
            "an arm after the `_` arm",
            "fn main() { match \"x\" { _ => {}; \"x\" => {}; }; };",
            "the `_` arm must be last",
        ),
        (
            "a second `_` arm",
            "fn main() { match \"x\" { \"x\" => {}; _ => {}; _ => {}; }; };",
            "the `_` arm must be last",
        ),
    ]);
}

#[test]
fn parses_a_closure() {
    let file = parse("fn main() { let f: fn(int) -> int = fn(x: int) -> int { return x; }; };");
    let Declaration::Function(function) = declaration(&file, 0) else {
        panic!("expected a function");
    };
    let Statement::Binding(binding) = &function.body[0] else {
        panic!("expected a binding");
    };
    assert!(matches!(binding.value, Expression::Closure { .. }));
}

#[test]
fn parses_a_no_value_function_type() {
    let file = parse("fn main() { let go: fn(txt) = fn(name: txt) { }; };");
    let Declaration::Function(function) = declaration(&file, 0) else {
        panic!("expected a function");
    };
    let Statement::Binding(binding) = &function.body[0] else {
        panic!("expected a binding");
    };
    let TypeExpr::Function { returns, .. } = &binding.ty else {
        panic!("expected a function type");
    };
    assert!(returns.is_none(), "a `fn(txt)` type returns no value");
}

#[test]
fn parses_a_method_function() {
    let file = parse("fn len<T>(self: Vec<T>) -> int { return 0; };");
    let Item::Declaration(Declaration::Function(function)) = &file.items[0] else {
        panic!("expected a function");
    };
    assert_eq!(function.type_parameters, vec!["T".to_owned()]);
    assert_eq!(function.parameters[0].name, "self");
    assert_eq!(function.name, "len");
}
