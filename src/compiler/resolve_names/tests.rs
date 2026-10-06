use super::resolve_names;
use crate::model::{DeclarationKind, Expression, Program, Statement};

fn linked(files: &[(&str, &str)], entry: &str) -> Result<Program, String> {
    let directory = tempfile::tempdir().expect("temp dir");
    for (name, source) in files {
        let path = directory.path().join(name);
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
        ("root main", &[("main.kiru", "fn main() {};")], "main.kiru"),
        (
            "root main with a record parameter",
            &[("main.kiru", "fn main(args<rec>) {};")],
            "main.kiru",
        ),
        (
            "cross-file call",
            &[
                (
                    "main.kiru",
                    "import \"tasks.kiru\";\nfn main() { tasks::go(); };",
                ),
                ("tasks.kiru", "mod tasks { fn go() {}; };"),
            ],
            "main.kiru",
        ),
        (
            "qualified own namespace",
            &[
                (
                    "main.kiru",
                    "import \"app.kiru\";\nfn main() { app::go(); };",
                ),
                (
                    "app.kiru",
                    "mod app { fn helper() {}; fn go() { app::helper(); }; };",
                ),
            ],
            "main.kiru",
        ),
        (
            "main declared by another file",
            &[
                ("main.kiru", "import \"helper.kiru\";"),
                ("helper.kiru", "fn main() {};"),
            ],
            "main.kiru",
        ),
        (
            "a transitively imported namespace is reachable",
            &[
                (
                    "main.kiru",
                    "import \"middle.kiru\";\nfn main() { leaf::go(); };",
                ),
                ("middle.kiru", "import \"leaf.kiru\";"),
                ("leaf.kiru", "mod leaf { fn go() {}; };"),
            ],
            "main.kiru",
        ),
    ]);
    expect_rejections(&[
        (
            "main with a text parameter",
            &[("main.kiru", "fn main(args<txt>) {};")],
            "main.kiru",
            "`main`'s parameter must be declared `rec`",
        ),
        (
            "main only inside a module",
            &[("main.kiru", "mod pipeline { fn main() {}; };")],
            "main.kiru",
            "the program has no root `main` function",
        ),
        (
            "main only in another namespace",
            &[
                ("main.kiru", "import \"tools.kiru\";"),
                ("tools.kiru", "mod tools { fn main() {}; };"),
            ],
            "main.kiru",
            "the program has no root `main` function",
        ),
        (
            "duplicate root main",
            &[
                ("main.kiru", "import \"helper.kiru\";\nfn main() {};"),
                ("helper.kiru", "fn main() {};"),
            ],
            "main.kiru",
            "`main` is declared more than once in this namespace",
        ),
        (
            "main with two parameters",
            &[("main.kiru", "fn main(a<rec>, b<rec>) {};")],
            "main.kiru",
            "`main` takes at most one parameter",
        ),
        (
            "std namespace",
            &[("main.kiru", "mod std { fn main() {}; };")],
            "main.kiru",
            "`std` is reserved and cannot be declared",
        ),
    ]);
}

#[test]
fn name_and_binding_rules() {
    expect_accepts(&[
        (
            "shadowing a visible module value",
            &[(
                "main.kiru",
                "let top<txt> = \"a\";\nfn main() { let top<txt> = \"b\"; std::print(top); };",
            )],
            "main.kiru",
        ),
        (
            "case arms reuse a name",
            &[(
                "main.kiru",
                "fn main() {\nmatch \"a\" {\n\"a\" => { let x<txt> = \"1\"; };\n\"b\" => { let x<txt> = \"2\"; };\n_ => { let x<txt> = \"3\"; };\n};\n};",
            )],
            "main.kiru",
        ),
    ]);
    expect_rejections(&[
        (
            "unknown name",
            &[("main.kiru", "fn main() { nope(); };")],
            "main.kiru",
            "unknown name `nope`",
        ),
        (
            "forward reference",
            &[(
                "main.kiru",
                "let x<txt> = y;\nlet y<txt> = \"a\";\nfn main() {};",
            )],
            "main.kiru",
            "`y` is declared after this point",
        ),
        (
            "recursion",
            &[("main.kiru", "fn f() { f(); };\nfn main() {};")],
            "main.kiru",
            "a function cannot reference itself",
        ),
        (
            "mutual recursion",
            &[(
                "main.kiru",
                "fn a() { b(); };\nfn b() { a(); };\nfn main() {};",
            )],
            "main.kiru",
            "`b` is declared after this point",
        ),
        (
            "duplicate parameters",
            &[("main.kiru", "fn f(a<txt>, a<txt>) {};\nfn main() {};")],
            "main.kiru",
            "`a` is declared more than once",
        ),
        (
            "redeclaring a local",
            &[(
                "main.kiru",
                "fn main() { let x<txt> = \"a\"; let x<txt> = \"b\"; };",
            )],
            "main.kiru",
            "`x` is declared more than once",
        ),
        (
            "shadowing a parameter",
            &[(
                "main.kiru",
                "fn f(x<txt>) { let x<txt> = \"a\"; };\nfn main() {};",
            )],
            "main.kiru",
            "`x` is declared more than once",
        ),
    ]);
}

#[test]
fn a_value_and_a_function_may_share_a_name() {
    let program = accept(
        &[(
            "main.kiru",
            "fn build() {};\nlet build<txt> = \"b\";\nfn main() { std::print(build); build(); };",
        )],
        "main.kiru",
    );
    let root = program.namespace(program.root());
    let value = root.value("build").expect("a value named build");
    let function = root.function("build").expect("a function named build");
    assert_ne!(value, function);
    assert!(matches!(
        program.declaration(function).kind,
        DeclarationKind::Function(_)
    ));
}

#[test]
fn calls_are_edges_to_the_callee() {
    let program = accept(
        &[
            (
                "main.kiru",
                "import \"tasks.kiru\";\nfn main() { tasks::go(); };",
            ),
            ("tasks.kiru", "mod tasks { fn go() {}; };"),
        ],
        "main.kiru",
    );
    let entry = program.declaration(program.entry);
    let DeclarationKind::Function(function) = &entry.kind else {
        panic!("the entry is a function");
    };
    let Statement::Expression(Expression::Call { callee, .. }) = &function.body[0] else {
        panic!("the body calls a function");
    };
    assert_eq!(program.declaration(*callee).name, "go");
}
