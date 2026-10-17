use super::resolve_names;
use crate::model::{Expression, Program, Statement};

fn linked(files: &[(&str, &str)], entry: &str) -> Result<Program, String> {
    let directory = tempfile::tempdir().expect("temp dir");
    for (name, source) in files {
        let path = directory.path().join(name);
        std::fs::create_dir_all(path.parent().expect("has a parent")).expect("create directory");
        std::fs::write(&path, source).expect("write file");
    }
    let program =
        crate::compiler::load_files::load_files(&directory.path().join(entry)).expect("loads");
    resolve_names(&program).map_err(|diagnostic| diagnostic.message)
}

fn accept(files: &[(&str, &str)], entry: &str) -> Program {
    linked(files, entry).expect("links")
}

/// One acceptance row: a case name, the files to write, and the entry file.
type AcceptCase<'a> = (&'a str, &'a [(&'a str, &'a str)], &'a str);

/// One rejection row: an acceptance row plus the expected error message.
type RejectCase<'a> = (&'a str, &'a [(&'a str, &'a str)], &'a str, &'a str);

/// Run every acceptance case, reporting all failures at once so one run names
/// every case that broke rather than only the first.
fn expect_accepts(cases: &[AcceptCase<'_>]) {
    let mut failures = Vec::new();
    for &(name, files, entry) in cases {
        if let Err(message) = linked(files, entry) {
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
fn expect_rejections(cases: &[RejectCase<'_>]) {
    let mut failures = Vec::new();
    for &(name, files, entry, expected) in cases {
        match linked(files, entry) {
            Ok(_) => failures.push(format!("{name}: expected `{expected}`, but it linked")),
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
fn entry_and_namespace_rules() {
    expect_accepts(&[
        (
            "root main",
            &[("main.kiru", "use std;\nfn main() {};")],
            "main.kiru",
        ),
        (
            "root main with a words parameter",
            &[(
                "main.kiru",
                "use std;\nuse std::dynamic::Vec;\nfn main(words: Vec<txt>) {};",
            )],
            "main.kiru",
        ),
        (
            "a file in a subdirectory is its own namespace",
            &[
                (
                    "main.kiru",
                    "use std;\nuse root::tasks;\nfn main() { tasks::go(); };",
                ),
                ("tasks/go.kiru", "pub fn go() {};"),
            ],
            "main.kiru",
        ),
        (
            "a sibling file shares the root namespace",
            &[
                ("main.kiru", "use std;\nfn main() { helper(); };"),
                ("helper.kiru", "fn helper() {};"),
            ],
            "main.kiru",
        ),
        (
            "main declared by another file in the root",
            &[("main.kiru", "use std;"), ("helper.kiru", "fn main() {};")],
            "main.kiru",
        ),
    ]);
    expect_rejections(&[
        (
            "main with a text parameter",
            &[("main.kiru", "use std;\nfn main(args: txt) {};")],
            "main.kiru",
            "`main`'s parameter must be declared `std::dynamic::Vec<txt>`",
        ),
        (
            "main only in a sub-namespace",
            &[
                ("main.kiru", "use std;\nuse root::tools;"),
                ("tools/mod.kiru", "pub fn main() {};"),
            ],
            "main.kiru",
            "the program has no root `main` function",
        ),
        (
            "duplicate root main",
            &[
                ("main.kiru", "use std;\nfn main() {};"),
                ("helper.kiru", "fn main() {};"),
            ],
            "main.kiru",
            "`main` is declared more than once in this namespace",
        ),
        (
            "main with two parameters",
            &[("main.kiru", "use std;\nfn main(a: txt, b: txt) {};")],
            "main.kiru",
            "`main` takes at most one parameter",
        ),
    ]);
}

#[test]
fn a_namespace_scoped_use_covers_the_directory() {
    // A `use` is namespace-scoped: any file in the directory may write it, and
    // it covers every declaration of the namespace. `a.kiru` writes the use;
    // `b.kiru` reads `Option` bare through it.
    accept(
        &[
            (
                "main.kiru",
                "use std;\nuse root::shared;\nfn main() { shared::g(); };",
            ),
            (
                "shared/a.kiru",
                "use std::Option;\npub fn f() -> Option<int> { return Option.none; };",
            ),
            (
                "shared/b.kiru",
                "pub fn g() -> Option<int> { return f(); };",
            ),
        ],
        "main.kiru",
    );
}

#[test]
fn a_method_needs_its_namespace_used() {
    // A method joins dispatch only when its namespace is `use`d. The hint names
    // the namespace to add. A name-`use` of the type alone reaches the type but
    // not its methods.
    expect_rejections(&[(
        "a Vec method without `use std::dynamic;`",
        &[(
            "main.kiru",
            "use std;\nuse std::dynamic::Vec;\nfn f(xs: Vec<int>) { let n: int = xs.len(); };\nfn main() {};",
        )],
        "main.kiru",
        "`len` is declared in `std::dynamic`; add `use std::dynamic;` to reach it",
    )]);
    accept(
        &[(
            "main.kiru",
            "use std;\nuse std::dynamic;\nuse std::dynamic::Vec;\nfn main() { let xs: Vec<int> = Vec.new().push(1).push(2); let n: int = xs.len(); };",
        )],
        "main.kiru",
    );
}

#[test]
fn vec_is_a_declared_type_in_std_dynamic() {
    // `Vec` is no longer an intrinsic and no longer bare: a bare `Vec` names
    // nothing, and the type is reached as `dynamic::Vec` after
    // `use std::dynamic;`, or as `Vec` after `use std::dynamic::Vec;`.
    expect_rejections(&[
        (
            "a bare Vec without a use",
            &[(
                "main.kiru",
                "use std;\nfn f(xs: Vec<int>) {};\nfn main() {};",
            )],
            "main.kiru",
            "unknown type `Vec`",
        ),
        (
            "a bare Vec receiver without a use",
            &[(
                "main.kiru",
                "use std;\nfn main() { let xs: dynamic::Vec<int> = Vec.new(); };",
            )],
            "main.kiru",
            "unknown name `Vec`",
        ),
    ]);
    accept(
        &[(
            "main.kiru",
            "use std;\nuse std::dynamic;\nfn main() { let xs: dynamic::Vec<int> = dynamic::Vec.new(); };",
        )],
        "main.kiru",
    );
    accept(
        &[(
            "main.kiru",
            "use std;\nuse std::dynamic;\nuse std::dynamic::Vec;\nfn main() { let xs: Vec<int> = Vec.new(); };",
        )],
        "main.kiru",
    );
}

#[test]
fn constructors_are_reached_through_the_type() {
    // A function with no parameters is reachable through the type it returns:
    // `Vec.new()` finds the vec constructor and `Map.new()` the map
    // constructor, both declared in `std::dynamic`.
    let program = accept(
        &[(
            "main.kiru",
            "use std;\nuse std::dynamic;\nuse std::dynamic::Vec;\nuse std::dynamic::Map;\nuse std::dynamic::Vec;\nfn main() { let xs: Vec<int> = Vec.new(); let m: Map<txt, int> = Map.new(); };",
        )],
        "main.kiru",
    );
    let root = program
        .namespace_at(&["root".to_owned()])
        .expect("the program root exists");
    let main = program
        .namespace(root)
        .get("main")
        .first()
        .copied()
        .expect("main exists");
    let body = &program
        .declaration(main)
        .function()
        .expect("main is a function")
        .body;
    let mut selected = Vec::new();
    for statement in body {
        let Statement::Bind { value, .. } = statement else {
            panic!("main binds two constructors");
        };
        let Expression::Call {
            callee, arguments, ..
        } = value
        else {
            panic!("the initializer is a call");
        };
        assert!(arguments.is_empty(), "a constructor takes no arguments");
        let Expression::Reference { declaration, .. } = callee.as_ref() else {
            panic!("the callee is a name");
        };
        selected.push(*declaration);
    }
    assert_eq!(selected.len(), 2);
    assert_ne!(
        selected[0], selected[1],
        "each type selects its own constructor"
    );
    for declaration in selected {
        assert_eq!(program.declaration(declaration).name, "new");
    }
}

#[test]
fn a_pub_let_is_rejected() {
    expect_rejections(&[(
        "a public module value",
        &[(
            "main.kiru",
            "use std;\npub let x: txt = \"a\";\nfn main() {};",
        )],
        "main.kiru",
        "a module value cannot be `pub`; it is private to its namespace",
    )]);
}

#[test]
fn a_module_value_cycle_is_rejected() {
    expect_rejections(&[(
        "two values that read each other",
        &[(
            "main.kiru",
            "use std;\nlet x: int = y;\nlet y: int = x;\nfn main() {};",
        )],
        "main.kiru",
        "`x` is part of a cycle among module values",
    )]);
    // An order-free forward reference resolves: a value may read a later one.
    accept(
        &[(
            "main.kiru",
            "use std;\nlet derived: txt = \"@(base)b\";\nlet base: txt = \"a\";\nfn main() {};",
        )],
        "main.kiru",
    );
}

#[test]
fn an_order_free_forward_reference_resolves() {
    accept(
        &[(
            "main.kiru",
            "use std;\nfn main() { std::print(banner()); };\nfn banner() -> txt { return \"x\"; };",
        )],
        "main.kiru",
    );
}

#[test]
fn a_use_after_a_declaration_is_a_parse_error() {
    let directory = tempfile::tempdir().expect("temp dir");
    let path = directory.path().join("main.kiru");
    std::fs::write(&path, "fn f() {};\nuse std;\n").expect("write file");
    let error = crate::compiler::load_files::load_files(&path)
        .expect_err("a use after a declaration fails");
    assert!(
        error
            .message
            .contains("`use` must come before any declaration"),
        "{}",
        error.message
    );
}

#[test]
fn an_unresolvable_use_is_rejected_at_its_span() {
    // A `use` names a namespace or a declaration; a path that reaches neither
    // is a compile error naming the path and pointing at the `use`.
    let source = "use std;\nuse std::tex;\nfn main() {};\n";
    let directory = tempfile::tempdir().expect("temp dir");
    let path = directory.path().join("main.kiru");
    std::fs::write(&path, source).expect("write file");
    let loaded = crate::compiler::load_files::load_files(&path).expect("loads");
    let diagnostic = resolve_names(&loaded).expect_err("the use is rejected");
    assert_eq!(
        diagnostic.message,
        "`std::tex` names neither a namespace nor a declaration"
    );
    // The span covers the `use std::tex;` line of the file that wrote it.
    assert_eq!(diagnostic.span, crate::syntax::Span::new(9, 22));
    let rendered = diagnostic.render(source);
    assert!(rendered.contains("main.kiru:2:1: error:"), "{rendered}");
}

#[test]
fn name_and_binding_rules() {
    expect_accepts(&[
        (
            "shadowing a visible module value",
            &[(
                "main.kiru",
                "use std;\nlet top: txt = \"a\";\nfn main() { let top: txt = \"b\"; std::print(top); };",
            )],
            "main.kiru",
        ),
        (
            "case arms reuse a name",
            &[(
                "main.kiru",
                "use std;\nfn main() {\nmatch \"a\" {\n\"a\" => { let x: txt = \"1\"; };\n\"b\" => { let x: txt = \"2\"; };\n_ => { let x: txt = \"3\"; };\n};\n};",
            )],
            "main.kiru",
        ),
    ]);
    expect_rejections(&[
        (
            "unknown name",
            &[("main.kiru", "use std;\nfn main() { nope(); };")],
            "main.kiru",
            "unknown name `nope`",
        ),
        (
            "duplicate parameters",
            &[(
                "main.kiru",
                "use std;\nfn f(a: txt, a: txt) {};\nfn main() {};",
            )],
            "main.kiru",
            "`a` is declared more than once",
        ),
        (
            "redeclaring a local",
            &[(
                "main.kiru",
                "use std;\nfn main() { let x: txt = \"a\"; let x: txt = \"b\"; };",
            )],
            "main.kiru",
            "`x` is declared more than once",
        ),
        (
            "shadowing a parameter",
            &[(
                "main.kiru",
                "use std;\nfn f(x: txt) { let x: txt = \"a\"; };\nfn main() {};",
            )],
            "main.kiru",
            "`x` is declared more than once",
        ),
    ]);
}

#[test]
fn a_duplicate_name_is_rejected() {
    expect_rejections(&[(
        "a value and a function share a name",
        &[(
            "main.kiru",
            "use std;\nfn build() {};\nlet build: txt = \"b\";\nfn main() { std::print(build); };",
        )],
        "main.kiru",
        "`build` is declared more than once in this namespace",
    )]);
}

#[test]
fn a_loop_item_keeps_its_element_type_for_dispatch() {
    // A method call on a loop item dispatches on the item's declared type.
    accept(
        &[(
            "main.kiru",
            "use std;\n\
             use std::dynamic;\n\
             use std::dynamic::Vec;\n\
             rec Service { name: txt };\n\
             fn line(self: Service) -> txt { return self.name; };\n\
             fn main() {\n\
             let api: Service = { name = \"api\" };\n\
             let services: Vec<Service> = Vec.new().push(api);\n\
             let index: int = 0;\n\
             let count: int = services.len();\n\
             for {\n\
             match index < count { false => { break; }; true => {}; };\n\
             let service: Service = services.at(index);\n\
             std::print(service.line());\n\
             index = index + 1;\n\
             };\n\
             };",
        )],
        "main.kiru",
    );
}

#[test]
fn a_method_name_may_repeat_per_receiver_head() {
    let program = accept(
        &[(
            "main.kiru",
            "use std;\n\
             rec A { n: int };\nrec B { n: int };\n\
             fn describe(self: A) -> txt { return \"a\"; };\n\
             fn describe(self: B) -> txt { return \"b\"; };\n\
             fn main() {\n\
             let a: A = { n = 1 };\nlet b: B = { n = 2 };\n\
             std::print(a.describe());\nstd::print(b.describe());\n\
             };",
        )],
        "main.kiru",
    );
    let root = program
        .namespace_at(&["root".to_owned()])
        .expect("the program root exists");
    // One name holds both declarations: the receiver's head tells them apart.
    let family = program.namespace(root).get("describe").to_vec();
    assert_eq!(family.len(), 2, "both methods share the name `describe`");
    let main = program
        .namespace(root)
        .get("main")
        .first()
        .copied()
        .expect("main exists");
    let body = &program
        .declaration(main)
        .function()
        .expect("main is a function")
        .body;
    // The last two statements print `a.describe()` and `b.describe()`. Each
    // desugars to a call whose callee is one of the two methods.
    let mut selected = Vec::new();
    for statement in &body[2..] {
        let Statement::Expression(Expression::Call { arguments, .. }) = statement else {
            panic!("the statement is a print call");
        };
        let Expression::Call { callee, .. } = &arguments[0] else {
            panic!("the print argument is a method call");
        };
        let Expression::Reference { declaration, .. } = callee.as_ref() else {
            panic!("the method callee is a name");
        };
        selected.push(*declaration);
    }
    assert_ne!(
        selected[0], selected[1],
        "each receiver selects its own method"
    );
    assert!(
        family.contains(&selected[0]) && family.contains(&selected[1]),
        "both calls select from the one family"
    );
    // A direct call selects by its first argument's head, through the method
    // pool, so the family is callable both ways.
    expect_accepts(&[(
        "a direct call selects by its first argument's head",
        &[(
            "main.kiru",
            "use std;\n\
             rec A { n: int };\nrec B { n: int };\n\
             fn describe(self: A) -> txt { return \"a\"; };\n\
             fn describe(self: B) -> txt { return \"b\"; };\n\
             fn main() {\n\
             let a: A = { n = 1 };\nlet b: B = { n = 2 };\n\
             std::print(describe(a));\nstd::print(describe(b));\n\
             };",
        )],
        "main.kiru",
    )]);
    expect_rejections(&[
        (
            "two methods for one receiver head",
            &[(
                "main.kiru",
                "use std;\n\
                 rec A { n: int };\n\
                 fn describe(self: A) -> txt { return \"a\"; };\n\
                 fn describe(self: A) -> txt { return \"b\"; };\n\
                 fn main() {};",
            )],
            "main.kiru",
            "`describe` is declared more than once in this namespace",
        ),
        (
            "a bare name for a method family",
            &[(
                "main.kiru",
                "use std;\n\
                 rec A { n: int };\nrec B { n: int };\n\
                 fn describe(self: A) -> txt { return \"a\"; };\n\
                 fn describe(self: B) -> txt { return \"b\"; };\n\
                 fn main() { let f: fn(A) -> txt = describe; };",
            )],
            "main.kiru",
            "`describe` is ambiguous for this value",
        ),
    ]);
}

#[test]
fn calls_are_edges_to_the_callee() {
    let program = accept(
        &[
            (
                "main.kiru",
                "use std;\nuse root::tasks;\nfn main() { tasks::go(); };",
            ),
            ("tasks/go.kiru", "pub fn go() {};"),
        ],
        "main.kiru",
    );
    let entry = program.declaration(program.entry);
    let function = entry.function().expect("the entry is a function");
    let Statement::Expression(Expression::Call { callee, .. }) = &function.body[0] else {
        panic!("the body calls a function");
    };
    let Expression::Reference { declaration, .. } = callee.as_ref() else {
        panic!("the callee is a name");
    };
    assert_eq!(program.declaration(*declaration).name, "go");
}

#[test]
fn a_named_function_and_a_closure_are_the_same_value() {
    let program = accept(
        &[(
            "main.kiru",
            "use std;\n\
             fn double(x: int) -> int { return x * 2; };\n\
             fn main() { let f: fn(int) -> int = fn(x: int) -> int { return x + 1; }; };",
        )],
        "main.kiru",
    );
    let root = program
        .namespace_at(&["root".to_owned()])
        .expect("the program root exists");
    let double = program
        .namespace(root)
        .get("double")
        .first()
        .copied()
        .expect("double exists");
    assert!(
        program.declaration(double).function().is_some(),
        "a named function's value is a function expression"
    );
    let main = program
        .namespace(root)
        .get("main")
        .first()
        .copied()
        .expect("main exists");
    let body = &program
        .declaration(main)
        .function()
        .expect("main is a function")
        .body;
    let Statement::Bind { value, .. } = &body[0] else {
        panic!("main binds a closure");
    };
    assert!(
        matches!(value, Expression::Function(_)),
        "a closure literal is the same function expression"
    );
}
