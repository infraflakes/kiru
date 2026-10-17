use super::resolve_names;
use crate::model::{Expression, Program, Statement};

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
        (
            "root main",
            &[(
                "main.kiru",
                "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn main() {};",
            )],
            "main.kiru",
        ),
        (
            "root main with a words parameter",
            &[(
                "main.kiru",
                "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn main(words: Vec<txt>) {};",
            )],
            "main.kiru",
        ),
        (
            "cross-file call",
            &[
                (
                    "main.kiru",
                    "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nimport \"tasks.kiru\";\nfn main() { tasks::go(); };",
                ),
                (
                    "tasks.kiru",
                    "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nmod tasks { pub fn go() {}; };",
                ),
            ],
            "main.kiru",
        ),
        (
            "qualified own namespace",
            &[
                (
                    "main.kiru",
                    "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nimport \"app.kiru\";\nfn main() { app::go(); };",
                ),
                (
                    "app.kiru",
                    "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nmod app { pub fn helper() {}; pub fn go() { app::helper(); }; };",
                ),
            ],
            "main.kiru",
        ),
        (
            "main declared by another file",
            &[
                (
                    "main.kiru",
                    "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nimport \"helper.kiru\";",
                ),
                (
                    "helper.kiru",
                    "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn main() {};",
                ),
            ],
            "main.kiru",
        ),
        (
            "a transitively imported namespace is reachable",
            &[
                (
                    "main.kiru",
                    "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nimport \"middle.kiru\";\nfn main() { leaf::go(); };",
                ),
                (
                    "middle.kiru",
                    "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nimport \"leaf.kiru\";",
                ),
                (
                    "leaf.kiru",
                    "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nmod leaf { pub fn go() {}; };",
                ),
            ],
            "main.kiru",
        ),
    ]);
    expect_rejections(&[
        (
            "main with a text parameter",
            &[(
                "main.kiru",
                "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn main(args: txt) {};",
            )],
            "main.kiru",
            "`main`'s parameter must be declared `Vec<txt>`",
        ),
        (
            "main only inside a module",
            &[(
                "main.kiru",
                "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nmod pipeline { fn main() {}; };",
            )],
            "main.kiru",
            "the program has no root `main` function",
        ),
        (
            "main only in another namespace",
            &[
                (
                    "main.kiru",
                    "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nimport \"tools.kiru\";",
                ),
                (
                    "tools.kiru",
                    "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nmod tools { fn main() {}; };",
                ),
            ],
            "main.kiru",
            "the program has no root `main` function",
        ),
        (
            "duplicate root main",
            &[
                (
                    "main.kiru",
                    "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nimport \"helper.kiru\";\nfn main() {};",
                ),
                (
                    "helper.kiru",
                    "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn main() {};",
                ),
            ],
            "main.kiru",
            "`main` is declared more than once in this namespace",
        ),
        (
            "main with two parameters",
            &[(
                "main.kiru",
                "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn main(a: txt, b: txt) {};",
            )],
            "main.kiru",
            "`main` takes at most one parameter",
        ),
        (
            "std namespace",
            &[(
                "main.kiru",
                "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nmod std { fn main() {}; };",
            )],
            "main.kiru",
            "`std` is reserved and cannot be declared",
        ),
        (
            "a use of a namespace imported later",
            &[
                (
                    "main.kiru",
                    "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nuse later;\nimport \"later.kiru\";\nfn main() {};",
                ),
                ("later.kiru", "mod later { pub fn f() {}; };"),
            ],
            "main.kiru",
            "`later` is declared in `later.kiru`, which is loaded after `main.kiru`",
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
                "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nlet top: txt = \"a\";\nfn main() { let top: txt = \"b\"; std::print(top); };",
            )],
            "main.kiru",
        ),
        (
            "case arms reuse a name",
            &[(
                "main.kiru",
                "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn main() {\nmatch \"a\" {\n\"a\" => { let x: txt = \"1\"; };\n\"b\" => { let x: txt = \"2\"; };\n_ => { let x: txt = \"3\"; };\n};\n};",
            )],
            "main.kiru",
        ),
    ]);
    expect_rejections(&[
        (
            "unknown name",
            &[(
                "main.kiru",
                "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn main() { nope(); };",
            )],
            "main.kiru",
            "unknown name `nope`",
        ),
        (
            "forward reference",
            &[(
                "main.kiru",
                "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nlet x: txt = y;\nlet y: txt = \"a\";\nfn main() {};",
            )],
            "main.kiru",
            "`y` is declared after this point",
        ),
        (
            "recursion",
            &[(
                "main.kiru",
                "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn f() { f(); };\nfn main() {};",
            )],
            "main.kiru",
            "a name cannot reference itself",
        ),
        (
            "mutual recursion",
            &[(
                "main.kiru",
                "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn a() { b(); };\nfn b() { a(); };\nfn main() {};",
            )],
            "main.kiru",
            "`b` is declared after this point",
        ),
        (
            "duplicate parameters",
            &[(
                "main.kiru",
                "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn f(a: txt, a: txt) {};\nfn main() {};",
            )],
            "main.kiru",
            "`a` is declared more than once",
        ),
        (
            "redeclaring a local",
            &[(
                "main.kiru",
                "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn main() { let x: txt = \"a\"; let x: txt = \"b\"; };",
            )],
            "main.kiru",
            "`x` is declared more than once",
        ),
        (
            "shadowing a parameter",
            &[(
                "main.kiru",
                "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn f(x: txt) { let x: txt = \"a\"; };\nfn main() {};",
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
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nfn build() {};\nlet build: txt = \"b\";\nfn main() { std::print(build); };",
        )],
        "main.kiru",
        "`build` is declared more than once in this namespace",
    )]);
}

#[test]
fn a_for_item_keeps_its_element_type_for_dispatch() {
    // A method call on a `for` item dispatches on the iterable's element type.
    // The item used to be assumed text at link time, so `service.line()` could
    // not find the method and the program did not compile.
    accept(
        &[(
            "main.kiru",
            "use std;\nuse std::text;\nuse std::vec;\n\
             rec Service { name: txt };\n\
             fn line(self: Service) -> txt { return self.name; };\n\
             fn main() {\n\
             let services: Vec<Service> = [{ name = \"api\" }];\n\
             for service in services { std::print(service.line()); };\n\
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
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\n\
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
    let root = program.root();
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
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\n\
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
                "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\n\
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
                "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\n\
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
                "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nimport \"tasks.kiru\";\nfn main() { tasks::go(); };",
            ),
            (
                "tasks.kiru",
                "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\nmod tasks { pub fn go() {}; };",
            ),
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
            "use std;\nuse std::text;\nuse std::vec;\nuse std::map;\n\
             fn double(x: int) -> int { return x * 2; };\n\
             fn main() { let f: fn(int) -> int = fn(x: int) -> int { return x + 1; }; };",
        )],
        "main.kiru",
    );
    let root = program.root();
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
