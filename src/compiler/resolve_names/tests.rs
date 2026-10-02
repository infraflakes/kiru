use super::link;
use crate::compiler::{DeclarationKind, Expression, Program, Statement};

fn linked(files: &[(&str, &str)], entry: &str) -> Result<Program, String> {
    let directory = tempfile::tempdir().expect("temp dir");
    for (name, source) in files {
        let path = directory.path().join(name);
        std::fs::write(&path, source).expect("write file");
    }
    let mut program = crate::compiler::loader::load(&directory.path().join(entry)).expect("loads");
    link(&mut program).map_err(|diagnostic| diagnostic.message)
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
            &[("main.kiru", "fn main(rec args) {};")],
            "main.kiru",
        ),
        (
            "cross-file call",
            &[
                (
                    "main.kiru",
                    "import \"tasks.kiru\";\nfn main() { tasks::go(); };",
                ),
                ("tasks.kiru", "module tasks;\nfn go() {};"),
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
                    "module app;\nfn helper() {};\nfn go() { app::helper(); };",
                ),
            ],
            "main.kiru",
        ),
    ]);
    expect_rejections(&[
        (
            "main with a text parameter",
            &[("main.kiru", "fn main(txt args) {};")],
            "main.kiru",
            "`main`'s parameter must be declared `rec`",
        ),
        (
            "main inside a module",
            &[("main.kiru", "module pipeline;\nfn main() {};")],
            "main.kiru",
            "`main` in the entry file must be declared in the root namespace",
        ),
        (
            "main declared by another file",
            &[
                ("main.kiru", "import \"helper.kiru\";"),
                ("helper.kiru", "fn main() {};"),
            ],
            "main.kiru",
            "the entry file has no `main` function",
        ),
        (
            "main in another namespace",
            &[
                ("main.kiru", "import \"tools.kiru\";"),
                ("tools.kiru", "module tools;\nfn main() {};"),
            ],
            "main.kiru",
            "the entry file has no `main` function",
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
            &[("main.kiru", "fn main(rec a, rec b) {};")],
            "main.kiru",
            "`main` takes at most one parameter",
        ),
        (
            "std namespace",
            &[("main.kiru", "module std;\nfn main() {};")],
            "main.kiru",
            "`std` is reserved and cannot be declared",
        ),
        (
            "transitively imported namespace",
            &[
                (
                    "main.kiru",
                    "import \"middle.kiru\";\nfn main() { leaf::go(); };",
                ),
                ("middle.kiru", "module middle;\nimport \"leaf.kiru\";"),
                ("leaf.kiru", "module leaf;\nfn go() {};"),
            ],
            "main.kiru",
            "namespace `leaf` is not imported",
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
                "txt top = \"a\";\nfn main() { txt top = \"b\"; std::print(top); };",
            )],
            "main.kiru",
        ),
        (
            "case arms reuse a name",
            &[(
                "main.kiru",
                "fn main() {\nswitch(\"a\") {\ncase(\"a\") { txt x = \"1\"; };\ncase(\"b\") { txt x = \"2\"; };\ndefault { txt x = \"3\"; };\n};\n};",
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
            &[("main.kiru", "txt x = y;\ntxt y = \"a\";\nfn main() {};")],
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
            &[("main.kiru", "fn f(txt a, txt a) {};\nfn main() {};")],
            "main.kiru",
            "`a` is declared more than once",
        ),
        (
            "redeclaring a local",
            &[("main.kiru", "fn main() { txt x = \"a\"; txt x = \"b\"; };")],
            "main.kiru",
            "`x` is declared more than once",
        ),
        (
            "shadowing a parameter",
            &[(
                "main.kiru",
                "fn f(txt x) { txt x = \"a\"; };\nfn main() {};",
            )],
            "main.kiru",
            "`x` is declared more than once",
        ),
        (
            "defer body shadows an enclosing name",
            &[(
                "main.kiru",
                "fn main() { txt x = \"a\"; defer { txt x = \"b\"; }; };",
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
            "fn build() {};\ntxt build = \"b\";\nfn main() { std::print(build); build(); };",
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
            ("tasks.kiru", "module tasks;\nfn go() {};"),
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
