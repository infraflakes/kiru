//! End to end tests for the parser grammar layers.

use super::parse_file;
use crate::syntax::ast::{
    Declaration, Expression, File, Item, Statement, TypeDeclarationKind, TypeExpr,
};

fn parse(source: &str) -> File {
    let parsed = parse_file(source).expect("source lexes");
    assert!(
        parsed.errors.is_empty(),
        "source parses: {:?}",
        parsed.errors
    );
    parsed.file
}

/// The name a written type names: a bare, unapplied single-segment path. A
/// built-in and a user type parse the same way.
fn written_name(expr: &TypeExpr) -> &str {
    match expr {
        TypeExpr::Named {
            path, arguments, ..
        } if arguments.is_empty() && path.len() == 1 => &path[0],
        other => panic!("expected a bare named type, found {other:?}"),
    }
}

/// The element type written inside a `Vec<...>` type, asserting the type is
/// spelled `Vec`.
fn vec_element(ty: &TypeExpr) -> &TypeExpr {
    let TypeExpr::Named {
        path, arguments, ..
    } = ty
    else {
        panic!("expected a named type, found {ty:?}");
    };
    assert_eq!(path, &["Vec".to_owned()]);
    assert_eq!(arguments.len(), 1, "a Vec type names its element type");
    &arguments[0]
}

/// The first syntax error a source earns, whether the lexer rejected it or
/// recovery recorded it. A lexer error is the only `Err`.
fn parse_error(source: &str) -> String {
    match parse_file(source) {
        Err(error) => error.message,
        Ok(parsed) => parsed
            .errors
            .first()
            .expect("source is rejected")
            .message
            .clone(),
    }
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
fn rejects_index_assignment() {
    // Brackets open a closure's parameter list only: `xs[i] = v` is not a form.
    let message = parse_error("fn main() { let xs: Vec<int> = Vec.new(); xs[0] = 1; };");
    assert_eq!(message, "expected `;` after the expression, found `[`");
}

#[test]
fn rejects_index_expressions() {
    let message = parse_error("fn f(xs: Vec<int>) -> int { return xs[0]; };");
    assert_eq!(message, "expected `;` after the return, found `[`");
}

#[test]
fn rejects_list_literals() {
    let message = parse_error("fn main() { let xs: Vec<int> = [1, 2]; };");
    assert_eq!(message, "expected a name in the parameter list, found `1`");
    let message = parse_error("fn main() { let xs: Vec<int> = []; };");
    assert_eq!(message, "expected `{` to open a block, found `;`");
}

#[test]
fn rejects_for_in() {
    // `for` has one form left: the bare `for { ... };`.
    let message = parse_error("fn main() { for x in xs { break; }; };");
    assert_eq!(message, "expected `{` to open a block, found `x`");
}

#[test]
fn parses_parameterized_vec_types() {
    let file = parse("fn f(xs: Vec<int>, nested: Vec<Vec<txt>>) -> Vec<txt> { return xs; };");
    let function = declaration(&file, 0)
        .function()
        .expect("expected a function");
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
    let function = declaration(&file, 0)
        .function()
        .expect("expected a function");
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
fn parses_a_use_of_a_namespace() {
    let file = parse("use std::dynamic;");
    let Item::Use(use_item) = &file.items[0] else {
        panic!("expected a use");
    };
    let segments: Vec<Vec<String>> = use_item
        .paths
        .iter()
        .map(|path| path.segments.clone())
        .collect();
    assert_eq!(segments, vec![vec!["std".to_owned(), "dynamic".to_owned()]]);
}

#[test]
fn parses_a_use_of_a_name() {
    let file = parse("use std::dynamic::Vec;\nfn f() {};");
    let Item::Use(use_item) = &file.items[0] else {
        panic!("expected a use");
    };
    let segments: Vec<Vec<String>> = use_item
        .paths
        .iter()
        .map(|path| path.segments.clone())
        .collect();
    assert_eq!(
        segments,
        vec![vec![
            "std".to_owned(),
            "dynamic".to_owned(),
            "Vec".to_owned()
        ]]
    );
    assert!(matches!(file.items[1], Item::Declaration(_)));
}

#[test]
fn parses_a_use_group() {
    // One level of braces names several things under one prefix: one `use`
    // item expands to one path per name.
    let file = parse("use std::dynamic::{HashMap, text_order};\nfn f() {};");
    let Item::Use(use_item) = &file.items[0] else {
        panic!("expected a use");
    };
    let segments: Vec<Vec<String>> = use_item
        .paths
        .iter()
        .map(|path| path.segments.clone())
        .collect();
    assert_eq!(
        segments,
        vec![
            vec!["std".to_owned(), "dynamic".to_owned(), "HashMap".to_owned()],
            vec![
                "std".to_owned(),
                "dynamic".to_owned(),
                "text_order".to_owned()
            ],
        ]
    );
    assert!(matches!(file.items[1], Item::Declaration(_)));
}

#[test]
fn rejects_a_use_group_that_is_not_one_level() {
    assert_eq!(
        parse_error("use std::dynamic::{Map, {Set}};"),
        "expected a name in a use group, found `{`"
    );
    assert_eq!(
        parse_error("use std::dynamic::{};"),
        "a use group must name at least one thing"
    );
}

#[test]
fn bare_names_are_values_and_parentheses_are_calls() {
    // A bare name is always a value; `name(...)` is always a call.
    let file = parse("fn main() { let f: fn() = go; go(); };");
    let function = declaration(&file, 0)
        .function()
        .expect("expected a function");
    let Statement::Binding(binding) = &function.body[0] else {
        panic!("expected a binding");
    };
    let Expression::Name { path, .. } = &binding.value else {
        panic!("expected a bare name, found {:?}", binding.value);
    };
    assert_eq!(path, &["go".to_owned()]);
    let Statement::Expression(Expression::Call { callee, .. }) = &function.body[1] else {
        panic!("expected a call statement");
    };
    assert_eq!(callee, &["go".to_owned()]);
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
    let function = declaration(&file, 0)
        .function()
        .expect("expected a function");
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
    let function = declaration(&file, 0)
        .function()
        .expect("expected a function");
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
    let function = declaration(&file, 0)
        .function()
        .expect("expected a function");
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
    let text_value = declaration(&file, 0)
        .function()
        .expect("expected a function");
    assert_eq!(
        text_value.return_type.as_ref().map(written_name),
        Some("txt")
    );
    let bool_value = declaration(&file, 1)
        .function()
        .expect("expected a function");
    assert_eq!(
        bool_value.return_type.as_ref().map(written_name),
        Some("bool")
    );
    let no_value = declaration(&file, 2)
        .function()
        .expect("expected a function");
    assert_eq!(no_value.return_type, None);
}

#[test]
fn parses_generic_type_parameters() {
    let file = parse("rec Pair<A, B> { first: A, second: B };");
    let type_decl = declaration(&file, 0)
        .type_declaration()
        .expect("expected a type declaration");
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
    let function = declaration(&file, 0)
        .function()
        .expect("expected a function");
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
    let function = declaration(&file, 0)
        .function()
        .expect("expected a function");
    assert_eq!(
        function.type_parameters,
        vec!["A".to_owned(), "B".to_owned()]
    );
}

#[test]
fn parses_a_function_type() {
    let file = parse("fn apply(f: fn(int) -> int, x: int) -> int { return x; };");
    let function = declaration(&file, 0)
        .function()
        .expect("expected a function");
    let TypeExpr::Function { parameters, .. } = &function.parameters[0].ty else {
        panic!("expected a function type");
    };
    assert_eq!(written_name(&parameters[0]), "int");
}

#[test]
fn parses_a_type_declaration() {
    let file = parse("rec Project { name: txt, count: int };");
    let declaration = declaration(&file, 0)
        .type_declaration()
        .expect("expected a type declaration");
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
fn rejects_a_bodyless_type_declaration() {
    // `type` is not a keyword: the intrinsics are built in, so a bodyless
    // declaration has no form and the word is an ordinary identifier.
    assert_eq!(
        parse_error("type txt;"),
        "expected a declaration, found `type`"
    );
    assert_eq!(
        parse_error("type Vec<T>;"),
        "expected a declaration, found `type`"
    );
}

#[test]
fn parses_an_enum_declaration() {
    let file = parse("enum Shape { Dot, Circle(int), Rect(int, int) };");
    let declaration = declaration(&file, 0)
        .type_declaration()
        .expect("expected a type declaration");
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
    let function = declaration(&file, 0)
        .function()
        .expect("expected a function");
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
    let function = declaration(&file, 0)
        .function()
        .expect("expected a function");
    assert!(matches!(function.body[0], Statement::Async { .. }));
    assert!(matches!(function.body[1], Statement::Wait { .. }));
}

#[test]
fn rejects_nested_async() {
    let message = parse_error("fn main() { async async work(); };");
    assert!(message.contains("expected an expression"), "{message}");
}

#[test]
fn rejects_a_leading_path_separator() {
    // A path begins with a name; the leading-`::` form does not exist.
    let message = parse_error("fn main() { std::print(::value); };");
    assert_eq!(message, "expected an expression, found `::`");
    let message = parse_error("fn f(x: ::std::T) {};");
    assert_eq!(message, "expected a type in the parameter list, found `::`");
}

#[test]
fn parses_value_no_value_and_panic_returns() {
    let file = parse("fn f(a: txt) -> txt { return a; };\nfn g() { return; };\nfn h() { panic; };");
    let value = declaration(&file, 0)
        .function()
        .expect("expected a function");
    assert!(matches!(
        value.body[0],
        Statement::Return { value: Some(_), .. }
    ));
    let empty = declaration(&file, 1)
        .function()
        .expect("expected a function");
    assert!(matches!(
        empty.body[0],
        Statement::Return { value: None, .. }
    ));
    let panics = declaration(&file, 2)
        .function()
        .expect("expected a function");
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
    let function = declaration(&file, 0)
        .function()
        .expect("expected a function");
    assert!(matches!(
        &function.body[0],
        Statement::FieldAssignment { field, .. } if field == "cmd"
    ));
}

#[test]
fn parses_bindings_and_field_assignment() {
    let file = parse("fn f(x: txt) { let r: P = { a = \"1\" }; r.a = \"2\"; };");
    let function = declaration(&file, 0)
        .function()
        .expect("expected a function");
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
            "a bare import is not a form",
            "import \"b.kiru\";\n",
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
    let file = parse("fn main() { let f: fn(int) -> int = [x: int] -> int { return x; }; };");
    let function = declaration(&file, 0)
        .function()
        .expect("expected a function");
    let Statement::Binding(binding) = &function.body[0] else {
        panic!("expected a binding");
    };
    assert!(matches!(binding.value, Expression::Closure { .. }));
}

#[test]
fn parses_a_no_value_function_type() {
    let file = parse("fn main() { let go: fn(txt) = [name: txt] { }; };");
    let function = declaration(&file, 0)
        .function()
        .expect("expected a function");
    let Statement::Binding(binding) = &function.body[0] else {
        panic!("expected a binding");
    };
    let TypeExpr::Function { returns, .. } = &binding.ty else {
        panic!("expected a function type");
    };
    assert!(returns.is_none(), "a `fn(txt)` type returns no value");
}

#[test]
fn parses_an_impl_block() {
    let file = parse(
        "impl Vec {\n\
           pub fn new<T>() -> Vec<T> { return Vec.new(); };\n\
           fn push<T>(self: Vec<T>, item: T) -> Vec<T> { return self; };\n\
         };",
    );
    let block = declaration(&file, 0)
        .impl_block()
        .expect("expected an impl block");
    assert_eq!(block.ty.named_path(), Some(&["Vec".to_owned()][..]));
    assert_eq!(block.functions.len(), 2);
    assert_eq!(block.functions[0].name, "new");
    assert!(block.functions[0].public);
    assert_eq!(block.functions[1].name, "push");
    assert!(!block.functions[1].public);
    assert_eq!(
        block.functions[1]
            .function()
            .expect("a member carries its function")
            .parameters[0]
            .name,
        "self"
    );
    // An empty block is a form: it files nothing.
    let file = parse("impl Vec {};");
    let block = declaration(&file, 0)
        .impl_block()
        .expect("expected an impl block");
    assert!(block.functions.is_empty());
}

#[test]
fn impl_block_errors() {
    assert_eq!(
        parse_error("impl Vec { let x: int = 1; };"),
        "expected a function in the impl block, found `let`"
    );
    assert_eq!(
        parse_error("pub impl Vec {};"),
        "expected a declaration, found `pub`"
    );
    assert_eq!(
        parse_error("impl Vec {}"),
        "expected `;` after the impl block, found the end of the file"
    );
    assert_eq!(
        parse_error("impl Vec. {};"),
        "expected `{` to open the impl block, found `.`"
    );
}

#[test]
fn parses_a_function_with_a_receiver_parameter() {
    // Outside an `impl` block, a function with a receiver-shaped first
    // parameter is an ordinary function: the block is what files it under a
    // type.
    let file = parse("fn len<T>(self: Vec<T>) -> int { return 0; };");
    let function = declaration(&file, 0)
        .function()
        .expect("expected a function");
    assert_eq!(function.type_parameters, vec!["T".to_owned()]);
    assert_eq!(function.parameters[0].name, "self");
    assert_eq!(function.name, "len");
}

#[test]
fn recovers_between_malformed_declarations() {
    let parsed = parse_file(
        "fn broken() -> {};\n\
         fn kept() {};\n\
         fn also_broken() -> {};",
    )
    .expect("source lexes");
    assert_eq!(parsed.errors.len(), 2, "both declarations are reported");
    assert!(
        parsed.file.items.iter().any(|item| matches!(
            item,
            Item::Declaration(declaration) if declaration.name == "kept"
        )),
        "the declaration between the errors is kept"
    );
}

#[test]
fn recovers_a_malformed_statement_and_keeps_the_rest() {
    let parsed = parse_file("fn main() { let broken: int = ; let kept: int = 1; broken(); };")
        .expect("source lexes");
    assert_eq!(parsed.errors.len(), 1);
    let function = declaration(&parsed.file, 0)
        .function()
        .expect("expected a function");
    assert_eq!(
        function.body.len(),
        2,
        "the statements after the broken one"
    );
    let Statement::Binding(binding) = &function.body[0] else {
        panic!("expected a binding");
    };
    assert_eq!(binding.name, "kept");
}

#[test]
fn recovers_past_an_error_inside_a_nested_block() {
    let parsed = parse_file(
        "fn before() {};\n\
         fn broken() { let value: int = ; };\n\
         fn after() {};",
    )
    .expect("source lexes");
    assert_eq!(parsed.errors.len(), 1);
    let names: Vec<&str> = parsed
        .file
        .items
        .iter()
        .filter_map(|item| match item {
            Item::Declaration(declaration) => Some(declaration.name.as_str()),
            Item::Use(_) => None,
        })
        .collect();
    assert_eq!(names, vec!["before", "broken", "after"]);
}

#[test]
fn an_unterminated_string_fails_the_lexer() {
    let error = parse_file("fn main() { let text: txt = \"unterminated; };")
        .expect_err("an unterminated string has no token boundary to recover to");
    assert_eq!(error.message, "unterminated string");
}
