use super::{Resolved, resolve_program};
use crate::compiler::checked_program;
use crate::compiler::model::{Expression, Program, Statement, TypeHead};

fn linked(files: &[(&str, &str)], entry: &str) -> Result<Program, String> {
    linked_with_entry(files, entry).map(|resolved| resolved.program)
}

/// Link the files and keep the entry the resolution selected.
fn linked_with_entry(files: &[(&str, &str)], entry: &str) -> Result<Resolved, String> {
    let directory = tempfile::tempdir().expect("temp dir");
    for (name, source) in files {
        let path = directory.path().join(name);
        std::fs::create_dir_all(path.parent().expect("has a parent")).expect("create directory");
        std::fs::write(&path, source).expect("write file");
    }
    let program = crate::compiler::load_files::load_files(
        &directory.path().join(entry),
        &crate::cli::DiskSource,
    )
    .map_err(|error| error.message)?;
    resolve_program(&program).map_err(|diagnostic| diagnostic.message)
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

/// Run one source through the whole pipeline, returning the first diagnostic
/// message. A resolution rule that also needs a body typed reads through this.
fn check(source: &str) -> Result<(), String> {
    crate::compiler::checked_files(&[("main.kiru", source)], "main.kiru").map(|_| ())
}

/// Run every acceptance case against the whole pipeline, reporting all
/// failures at once so one run names every case that broke.
fn expect_checked_accepts(cases: &[(&str, &str)]) {
    let mut failures = Vec::new();
    for &(name, source) in cases {
        if let Err(message) = check(source) {
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

/// Run every rejection case against the whole pipeline, collecting all
/// mismatches so one run reports every case that failed.
fn expect_checked_rejections(cases: &[(&str, &str, &str)]) {
    let mut failures = Vec::new();
    for &(name, source, expected) in cases {
        match check(source) {
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
    ]);
    expect_rejections(&[
        (
            "an entry file that declares no main, while a file beside it does",
            &[("main.kiru", "use std;"), ("helper.kiru", "fn main() {};")],
            "main.kiru",
            "the entry file declares no `main` function",
        ),
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
fn a_method_needs_no_use() {
    // A bound function is gated by `pub` alone, never by whether its namespace
    // is `use`d: `xs.len()` works with no `use` in the file at all.
    accept(
        &[(
            "main.kiru",
            "fn main() { let xs: std::dynamic::Vec<int> = std::dynamic::Vec.new(); xs = xs.push(1); let n: int = xs.len(); };",
        )],
        "main.kiru",
    );
    // A name-`use` of the type alone reaches the type and, through it, its
    // bound functions.
    accept(
        &[(
            "main.kiru",
            "use std::dynamic::Vec;\nfn main() { let xs: Vec<int> = Vec.new(); let n: int = xs.len(); };",
        )],
        "main.kiru",
    );
}

#[test]
fn absolute_paths_resolve_with_no_use() {
    // A path's first segment is a tree name, so `std::…` and `root::…` always
    // resolve: no `use` is needed for an absolute path.
    accept(
        &[(
            "main.kiru",
            "fn main() { std::print(\"no use at all\"); let xs: std::dynamic::Vec<int> = std::dynamic::Vec.new(); std::print(\"@(std::dynamic::len(xs))\"); };",
        )],
        "main.kiru",
    );
    accept(
        &[
            (
                "main.kiru",
                "use root::tasks;\nfn main() { std::print(tasks::go()); };",
            ),
            ("tasks/go.kiru", "pub fn go() -> txt { return \"x\"; };"),
        ],
        "main.kiru",
    );
}

#[test]
fn a_use_shortens_paths() {
    // A namespace-`use` gives the qualifier its last segment spells.
    accept(
        &[(
            "main.kiru",
            "use std::dynamic;\nfn main() { let xs: dynamic::Vec<int> = dynamic::Vec.new(); let n: int = dynamic::len(xs); };",
        )],
        "main.kiru",
    );
    // A type-`use` gives a bare type, and its bound functions come with it.
    accept(
        &[(
            "main.kiru",
            "use std::dynamic::Map;\nfn main() { let m: Map<txt, int> = Map.new(); let n: int = m.len(); };",
        )],
        "main.kiru",
    );
    // One level of braces names several things under one prefix.
    accept(
        &[(
            "main.kiru",
            "use std::dynamic::{Map, text_order};\nfn main() { let m: Map<txt, int> = Map.new(); let b: bool = text_order(\"a\", \"b\"); };",
        )],
        "main.kiru",
    );
}

#[test]
fn an_impl_bound_function_is_not_a_name() {
    // A function in an `impl` block is reachable only through its type: it is
    // not a namespace-level name, so a bare `push` means nothing and a `use`
    // of it is rejected.
    expect_rejections(&[
        (
            "a use of a method",
            &[(
                "main.kiru",
                "use std::dynamic;\nuse std::dynamic::push;\nfn main() {};",
            )],
            "main.kiru",
            "`std::dynamic::push` names neither a namespace nor a declaration",
        ),
        (
            "a bare method call",
            &[(
                "main.kiru",
                "use std::dynamic;\nfn main() { let xs: dynamic::Vec<int> = dynamic::Vec.new(); push(xs, 1); };",
            )],
            "main.kiru",
            "unknown name `push`",
        ),
        (
            "a bare method name as a value",
            &[(
                "main.kiru",
                "use std::dynamic;\nfn main() { let f: fn(dynamic::Vec<int>, int) -> dynamic::Vec<int> = push; };",
            )],
            "main.kiru",
            "unknown name `push`",
        ),
        (
            "a qualified method name as a value",
            &[(
                "main.kiru",
                "use std::dynamic;\nfn main() { let f: int = dynamic::push; };",
            )],
            "main.kiru",
            "unknown name `dynamic::push`",
        ),
    ]);
    // A qualified path reaches the bound function through the namespace, and
    // the method reaches it through the receiver.
    accept(
        &[(
            "main.kiru",
            "fn main() { let xs: std::dynamic::Vec<int> = std::dynamic::Vec.new(); xs = std::dynamic::push(xs, 1); xs = xs.push(2); };",
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
fn associated_functions_are_reached_through_the_type() {
    // A function in an `impl` block whose first parameter is not the block's
    // type is an associated function: `Vec.new()` finds the native vec `new`
    // and `Map.new()` the map one, both filed under their type.
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
        .get(&program.intern, "main")
        .expect("main exists");
    let body = &program
        .declaration(main)
        .function()
        .expect("main is a function")
        .body;
    let mut selected = Vec::new();
    for statement in body {
        let Statement::Bind { value, .. } = statement else {
            panic!("main binds two associated functions");
        };
        let Expression::Call {
            callee, arguments, ..
        } = value
        else {
            panic!("the initializer is a call");
        };
        assert!(arguments.is_empty(), "`new` takes no arguments");
        let Expression::Reference { declaration, .. } = callee.as_ref() else {
            panic!("the callee is a name");
        };
        selected.push(*declaration);
    }
    assert_eq!(selected.len(), 2);
    assert_ne!(
        selected[0], selected[1],
        "each type selects its own associated function"
    );
    for declaration in selected {
        assert_eq!(program.declaration(declaration).name, "new");
    }
}

#[test]
fn the_library_files_its_functions_under_their_types() {
    // The runtime's `Vec` rows name `Vec` themselves: `push` has it as its
    // first parameter, so it is a method, and `new` returns it, so it is an
    // associated function reached as `Vec.new()`.
    let program = accept(
        &[(
            "main.kiru",
            "use std;\nuse std::dynamic;\nuse std::dynamic::Vec;\nfn main() { let xs: Vec<int> = Vec.new(); xs = xs.push(1); };",
        )],
        "main.kiru",
    );
    let dynamic = program
        .namespace_at(&["std".to_owned(), "dynamic".to_owned()])
        .expect("std::dynamic exists");
    let namespace = program.namespace(dynamic);
    let list_type = program
        .list_type()
        .expect("the compiler registers the runtime list");
    let head = TypeHead::Named(list_type);
    let new = program
        .intern
        .resolve("new")
        .and_then(|symbol| namespace.associates.get(head, symbol))
        .expect("`new` is an associated function of Vec");
    assert_eq!(program.declaration(new).name, "new");
    assert!(
        namespace
            .methods
            .get(head, program.intern.resolve("new").expect("interned"))
            .is_none(),
        "`new` is not a method"
    );
    let push = program
        .intern
        .resolve("push")
        .and_then(|symbol| namespace.methods.get(head, symbol))
        .expect("`push` is a method of Vec");
    assert_eq!(program.declaration(push).name, "push");
    assert!(
        namespace
            .associates
            .get(head, program.intern.resolve("push").expect("interned"))
            .is_none(),
        "`push` is not an associated function"
    );
}

#[test]
fn impl_blocks_for_one_type_merge() {
    // Two blocks for one type merge the way two files make one namespace: a
    // method of one block calls a method of the other.
    accept(
        &[(
            "main.kiru",
            "use std;\nrec Point { x: int, y: int };\n\
             impl Point { pub fn sum(self: Point) -> int { return self.x + self.y; }; };\n\
             impl Point { pub fn scaled(self: Point, factor: int) -> int { return self.sum() * factor; }; };\n\
             fn main() { let p: Point = { x = 1, y = 2 }; std::print(\"@(p.scaled(3))\"); };",
        )],
        "main.kiru",
    );
    expect_rejections(&[(
        "a duplicate across two blocks",
        &[(
            "main.kiru",
            "use std;\nrec Point { x: int };\n\
             impl Point { pub fn sum(self: Point) -> int { return 0; }; };\n\
             impl Point { pub fn sum(self: Point) -> int { return 1; }; };\n\
             fn main() {};",
        )],
        "main.kiru",
        "`sum` is declared more than once in this namespace",
    )]);
}

#[test]
fn an_impl_block_files_its_functions_in_its_namespace() {
    // The block's functions are filed in the namespace of the file that wrote
    // it, and a bound function is gated by `pub` alone, so a caller reaches a
    // public method through the type.
    accept(
        &[
            (
                "main.kiru",
                "use std;\nuse root::m;\nfn main() { let p: m::P = { x = 1 }; std::print(\"@(p.sum())\"); };",
            ),
            (
                "m/mod.kiru",
                "pub rec P { x: int };\nimpl P { pub fn sum(self: P) -> int { return self.x; }; };",
            ),
        ],
        "main.kiru",
    );
    // A name-`use` of the type reaches the type and, through it, its methods.
    accept(
        &[
            (
                "main.kiru",
                "use std;\nuse root::m::P;\nfn main() { let p: P = { x = 1 }; std::print(\"@(p.sum())\"); };",
            ),
            (
                "m/mod.kiru",
                "pub rec P { x: int };\nimpl P { pub fn sum(self: P) -> int { return self.x; }; };",
            ),
        ],
        "main.kiru",
    );
}

#[test]
fn a_same_named_function_in_two_blocks_does_not_collide() {
    // `impl Map` and `impl Set` may both declare `of`: the two blocks file it
    // under different types, so neither owns the name alone.
    accept(
        &[(
            "main.kiru",
            "use std;\nuse std::dynamic;\nuse std::dynamic::Map;\nuse std::dynamic::Set;\nuse std::dynamic::Vec;\n\
             fn order(a: txt, b: txt) -> bool { return a < b; };\n\
             impl Map { pub fn of() -> Map<txt, int> { return Map.new(); }; };\n\
             impl Set { pub fn of() -> Set<txt> { return Set.from_items(Vec.new(), order); }; };\n\
             fn main() { let m: Map<txt, int> = Map.of(); let s: Set<txt> = Set.of(); std::print(\"@(m.len()) @(s.len())\"); };",
        )],
        "main.kiru",
    );
}

#[test]
fn a_plain_function_is_not_a_method() {
    // A function outside an `impl` block is never reached through a value: its
    // first parameter's type is irrelevant.
    expect_checked_rejections(&[(
        "a plain function with a matching first parameter",
        "use std;\nuse std::dynamic;\nuse std::dynamic::Vec;\nfn describe(xs: Vec<int>) -> int { return 0; };\nfn main() { let xs: Vec<int> = Vec.new(); xs.describe(); };",
        "no method `describe` for this value",
    )]);
}

#[test]
fn an_impl_block_with_an_unknown_target_is_rejected() {
    expect_rejections(&[(
        "an impl block for a missing type",
        &[(
            "main.kiru",
            "use std;\nimpl Missing { fn f() {}; };\nfn main() {};",
        )],
        "main.kiru",
        "unknown type `Missing`",
    )]);
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
        "expected a declaration, found `pub`",
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

/// The names of a program's module values in evaluation order: each is
/// evaluated after the values its initializer reads.
fn module_value_order(program: &Program) -> Vec<String> {
    program
        .module_values
        .iter()
        .map(|id| program.declaration(*id).name.clone())
        .collect()
}

/// The position of one module value in an evaluation order.
fn module_value_position(order: &[String], name: &str) -> usize {
    order
        .iter()
        .position(|entry| entry == name)
        .unwrap_or_else(|| panic!("`{name}` is a module value: {order:?}"))
}

#[test]
fn a_closure_in_an_initializer_does_not_order_the_value_after_what_it_mentions() {
    // A closure's body runs when it is called, not when the value that builds
    // it is made, so the module value it mentions is not a dependency of the
    // initializer. `derived` keeps its declaration position.
    let program = checked_program(
        "use std;\nfn apply(f: fn() -> int) -> int { return f(); };\nlet derived: int = apply(fn() -> int { return base; });\nlet base: int = 1;\nfn main() {};",
    );
    let order = module_value_order(&program);
    assert!(
        module_value_position(&order, "derived") < module_value_position(&order, "base"),
        "the closure body is deferred, so `derived` is not ordered after `base`: {order:?}"
    );
}

#[test]
fn a_call_to_a_function_valued_value_orders_it_after_what_that_value_reads() {
    // The callee is another module value: `derived` calls `reader`, and
    // `reader` reads `base`, so `derived` is evaluated after both.
    let program = checked_program(
        "use std;\nfn base_fn() -> int { return 1; };\nlet derived: int = reader();\nlet reader: fn() -> int = base;\nlet base: fn() -> int = base_fn;\nfn main() {};",
    );
    let order = module_value_order(&program);
    assert!(
        module_value_position(&order, "base") < module_value_position(&order, "reader"),
        "`reader` reads `base`, so it is ordered after it: {order:?}"
    );
    assert!(
        module_value_position(&order, "reader") < module_value_position(&order, "derived"),
        "`derived` calls `reader`, so it is ordered after it: {order:?}"
    );
}

#[test]
fn a_module_value_cycle_through_a_deferred_body_is_legal() {
    // A reference in a function literal body is evaluated when the value is
    // called, not when its initializer runs, so it is not a module-value
    // dependency and the mention creates no cycle.
    accept(
        &[(
            "main.kiru",
            "use std;\nfn apply(f: fn() -> int) -> int { return f(); };\nlet x: int = apply(fn() -> int { return x; });\nfn main() {};",
        )],
        "main.kiru",
    );
    accept(
        &[(
            "main.kiru",
            "use std;\nfn wrap(f: fn() -> int) -> fn() -> int { return f; };\nlet a: fn() -> int = wrap(fn() -> int { return b(); });\nlet b: fn() -> int = a;\nfn main() {};",
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
fn an_unresolvable_use_is_rejected_at_its_span() {
    // A `use` names a namespace or a declaration; a path that reaches neither
    // is a compile error naming the path and pointing at the `use`.
    let source = "use std;\nuse std::tex;\nfn main() {};\n";
    let directory = tempfile::tempdir().expect("temp dir");
    let path = directory.path().join("main.kiru");
    std::fs::write(&path, source).expect("write file");
    let loaded =
        crate::compiler::load_files::load_files(&path, &crate::cli::DiskSource).expect("loads");
    let diagnostic = match resolve_program(&loaded) {
        Ok(_) => panic!("the use is rejected"),
        Err(diagnostic) => diagnostic,
    };
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
             impl Service { pub fn line(self: Service) -> txt { return self.name; }; };\n\
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
             impl A { pub fn describe(self: A) -> txt { return \"a\"; }; };\n\
             impl B { pub fn describe(self: B) -> txt { return \"b\"; }; };\n\
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
    // The two methods share the name `describe`: the receiver's head tells
    // them apart, and a bound function is not a namespace-level name.
    let namespace = program.namespace(root);
    let a = namespace
        .type_id(&program.intern, "A")
        .expect("A is declared");
    let b = namespace
        .type_id(&program.intern, "B")
        .expect("B is declared");
    let a_symbol = program
        .intern
        .resolve("describe")
        .expect("describe is interned");
    let a_method = namespace
        .methods
        .get(TypeHead::Named(a), a_symbol)
        .expect("A has a describe method");
    let b_method = namespace
        .methods
        .get(TypeHead::Named(b), a_symbol)
        .expect("B has a describe method");
    assert_ne!(a_method, b_method, "each head has its own method");
    assert!(
        namespace.get(&program.intern, "describe").is_none(),
        "a bound function is not a name"
    );
    // A qualified call selects by its first argument's head, through the
    // namespace's method pool.
    expect_accepts(&[(
        "a qualified call selects by its first argument's head",
        &[(
            "main.kiru",
            "use std;\n\
             rec A { n: int };\nrec B { n: int };\n\
             impl A { pub fn describe(self: A) -> txt { return \"a\"; }; };\n\
             impl B { pub fn describe(self: B) -> txt { return \"b\"; }; };\n\
             fn main() {\n\
             let a: A = { n = 1 };\nlet b: B = { n = 2 };\n\
             std::print(root::describe(a));\nstd::print(root::describe(b));\n\
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
                 impl A { pub fn describe(self: A) -> txt { return \"a\"; }; pub fn describe(self: A) -> txt { return \"b\"; }; };\n\
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
                 impl A { pub fn describe(self: A) -> txt { return \"a\"; }; };\n\
                 impl B { pub fn describe(self: B) -> txt { return \"b\"; }; };\n\
                 fn main() { let f: fn(A) -> txt = describe; };",
            )],
            "main.kiru",
            "unknown name `describe`",
        ),
        (
            "a bare call reaches no bound function",
            &[(
                "main.kiru",
                "use std;\n\
                 rec A { n: int };\n\
                 impl A { pub fn describe(self: A) -> txt { return \"a\"; }; };\n\
                 fn main() { let a: A = { n = 1 }; std::print(describe(a)); };",
            )],
            "main.kiru",
            "unknown name `describe`",
        ),
    ]);
}

#[test]
fn calls_are_edges_to_the_callee() {
    let resolved = linked_with_entry(
        &[
            (
                "main.kiru",
                "use std;\nuse root::tasks;\nfn main() { tasks::go(); };",
            ),
            ("tasks/go.kiru", "pub fn go() {};"),
        ],
        "main.kiru",
    )
    .expect("links");
    let program = &resolved.program;
    let entry = program.declaration(resolved.entry);
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
        .get(&program.intern, "double")
        .expect("double exists");
    assert!(
        program.declaration(double).function().is_some(),
        "a named function's value is a function expression"
    );
    let main = program
        .namespace(root)
        .get(&program.intern, "main")
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

#[test]
fn name_and_call_rules() {
    expect_checked_accepts(&[
        (
            "a local shadows a module value; a qualified path reaches it",
            "use std;\nuse std::text;\nuse std::dynamic;\nuse std::process;\nuse std::Option;\nuse std::dynamic::Map;\nuse std::dynamic::Vec;\nuse root;\nlet value: txt = \"root\";\nfn main() {\nlet value: txt = \"local\";\nstd::print(value);\nstd::print(root::value);\n};",
        ),
        (
            "a function may call itself",
            "use std;\nuse std::text;\nuse std::dynamic;\nuse std::process;\nuse std::Option;\nuse std::dynamic::Map;\nuse std::dynamic::Vec;\nfn f() { f(); };\nfn main() {};",
        ),
    ]);
    expect_checked_rejections(&[
        (
            "a value and a function share a name",
            "use std;\nuse std::text;\nuse std::dynamic;\nuse std::process;\nuse std::Option;\nuse std::dynamic::Map;\nuse std::dynamic::Vec;\nfn build() -> txt { return \"built\"; };\nlet build: txt = \"text\";\nfn main() { std::print(build); };",
            "`build` is declared more than once in this namespace",
        ),
        (
            "function value where a text is expected",
            "use std;\nuse std::text;\nuse std::dynamic;\nuse std::process;\nuse std::Option;\nuse std::dynamic::Map;\nuse std::dynamic::Vec;\nfn f() {};\nfn main() { let x: txt = f; };",
            "expected txt, found fn()",
        ),
        (
            "calling a module value",
            "use std;\nuse std::text;\nuse std::dynamic;\nuse std::process;\nuse std::Option;\nuse std::dynamic::Map;\nuse std::dynamic::Vec;\nlet x: txt = \"a\";\nfn main() { x(); };",
            "`root::x` is a value and cannot be called",
        ),
        (
            "calling a local",
            "use std;\nuse std::text;\nuse std::dynamic;\nuse std::process;\nuse std::Option;\nuse std::dynamic::Map;\nuse std::dynamic::Vec;\nfn main() { let x: txt = \"a\"; x(); };",
            "`x` is a value and cannot be called",
        ),
        (
            "wrong arity",
            "use std;\nuse std::text;\nuse std::dynamic;\nuse std::process;\nuse std::Option;\nuse std::dynamic::Map;\nuse std::dynamic::Vec;\nfn f(a: txt) {};\nfn main() { f(); };",
            "`root::f` takes 1 argument, found 0",
        ),
        (
            "unknown absolute name",
            "use std;\nuse std::text;\nuse std::dynamic;\nuse std::process;\nuse std::Option;\nuse std::dynamic::Map;\nuse std::dynamic::Vec;\nuse root;\nlet value: txt = \"root\";\nfn main() { std::print(root::missing); };",
            "unknown name `root::missing`",
        ),
        (
            "value self reference",
            "use std;\nuse std::text;\nuse std::dynamic;\nuse std::process;\nuse std::Option;\nuse std::dynamic::Map;\nuse std::dynamic::Vec;\nlet x: txt = x;\nfn main() {};",
            "`x` is part of a cycle among module values",
        ),
    ]);
}
#[test]
fn method_rules() {
    expect_checked_accepts(&[
        (
            "a method on a Vec",
            "use std;\nuse std::text;\nuse std::dynamic;\nuse std::process;\nuse std::Option;\nuse std::dynamic::Map;\nuse std::dynamic::Vec;\nimpl Vec { pub fn tally<T>(self: Vec<T>) -> int { let n: int = 0; let index: int = 0; let total: int = self.len(); for { match index < total { false => { break; }; true => {}; }; n = n + 1; index = index + 1; }; return n; }; };\nfn main() { let xs: Vec<int> = Vec.new().push(1).push(2); let n: int = xs.tally(); };",
        ),
        (
            "a method on a declared type",
            "use std;\nuse std::text;\nuse std::dynamic;\nuse std::process;\nuse std::Option;\nuse std::dynamic::Map;\nuse std::dynamic::Vec;\nrec Point { x: int, y: int };\nimpl Point { pub fn sum(self: Point) -> int { return self.x + self.y; }; };\nfn main() { let p: Point = { x = 1, y = 2 }; let n: int = p.sum(); };",
        ),
        (
            "a method calling an earlier method",
            "use std;\nuse std::text;\nuse std::dynamic;\nuse std::process;\nuse std::Option;\nuse std::dynamic::Map;\nuse std::dynamic::Vec;\nimpl Vec { pub fn tally<T>(self: Vec<T>) -> int { return 0; }; pub fn head<T>(self: Vec<T>) -> int { return self.tally(); }; };\nfn main() { let xs: Vec<int> = Vec.new().push(1); let n: int = xs.head(); };",
        ),
        (
            "the receiver parameter's name is irrelevant",
            "use std;\nuse std::text;\nuse std::dynamic;\nuse std::process;\nuse std::Option;\nuse std::dynamic::Map;\nuse std::dynamic::Vec;\nimpl Vec { pub fn tally<T>(xs: Vec<T>) -> int { return 0; }; };\nfn main() { let xs: Vec<int> = Vec.new().push(1); let n: int = xs.tally(); };",
        ),
        (
            "an associated function is reached through the type",
            "use std;\nuse std::text;\nuse std::dynamic;\nuse std::process;\nuse std::Option;\nuse std::dynamic::Map;\nuse std::dynamic::Vec;\nrec Point { x: int };\nimpl Point { pub fn origin() -> Point { return { x = 0 }; }; };\nfn main() { let p: Point = Point.origin(); };",
        ),
        (
            "a method needs no use in the file",
            "fn main() { let xs: std::dynamic::Vec<int> = std::dynamic::Vec.new(); xs = xs.push(1); std::print(\"@(xs.len())\"); };",
        ),
        (
            "a qualified path reaches a bound function",
            "fn main() { let xs: std::dynamic::Vec<int> = std::dynamic::Vec.new(); xs = xs.push(1); std::print(\"@(std::dynamic::len(xs))\"); };",
        ),
        (
            "a bound function is a value through its type",
            "use std;\nuse std::text;\nuse std::dynamic;\nuse std::process;\nuse std::Option;\nuse std::dynamic::Map;\nuse std::dynamic::Vec;\nrec Point { x: int };\nimpl Point { pub fn origin() -> Point { return { x = 0 }; }; pub fn x_of(self: Point) -> int { return self.x; }; };\nfn main() { let make: fn() -> Point = Point.origin; let read: fn(Point) -> int = Point.x_of; let p: Point = make(); let n: int = read(p); std::print(\"@(n)\"); };",
        ),
        (
            "a plain native is a namespace-level name",
            "fn main() { std::print(std::syscalls::getcwd()); };",
        ),
    ]);
    expect_checked_rejections(&[
        (
            "an unknown method",
            "use std;\nuse std::text;\nuse std::dynamic;\nuse std::process;\nuse std::Option;\nuse std::dynamic::Map;\nuse std::dynamic::Vec;\nfn main() { let xs: Vec<int> = Vec.new().push(1).push(2); xs.nope(); };",
            "no method `nope` for this value",
        ),
        (
            "a bound function is not namespace-qualified",
            "fn main() { let xs: std::dynamic::Vec<int> = std::dynamic::new(); };",
            "unknown name `std::dynamic::new`",
        ),
        (
            "a bare call reaches no bound function",
            "fn main() { let xs: std::dynamic::Vec<int> = std::dynamic::Vec.new(); std::dynamic::push(xs, 1); push(xs, 2); };",
            "unknown name `push`",
        ),
        (
            "a use of a bound function",
            "use std::dynamic;\nuse std::dynamic::push;\nfn main() {};",
            "`std::dynamic::push` names neither a namespace nor a declaration",
        ),
        (
            "a duplicate name in one block",
            "use std;\nuse std::text;\nuse std::dynamic;\nuse std::process;\nuse std::Option;\nuse std::dynamic::Map;\nuse std::dynamic::Vec;\nimpl Vec { pub fn tally<T>(self: Vec<T>) -> int { return 0; }; pub fn tally<T>(self: Vec<T>) -> int { return 1; }; };\nfn main() {};",
            "`tally` is declared more than once in this namespace",
        ),
        (
            "a plain function is not a method",
            "use std;\nuse std::text;\nuse std::dynamic;\nuse std::process;\nuse std::Option;\nuse std::dynamic::Map;\nuse std::dynamic::Vec;\nfn tally(xs: Vec<txt>) -> int { return 0; };\nfn main() { let xs: Vec<int> = Vec.new().push(1); xs.tally(); };",
            "no method `tally` for this value",
        ),
        (
            "a method is not an associated function",
            "use std;\nuse std::text;\nuse std::dynamic;\nuse std::process;\nuse std::Option;\nuse std::dynamic::Map;\nuse std::dynamic::Vec;\nimpl Vec { pub fn tally<T>(self: Vec<T>) -> int { return 0; }; };\nfn main() { let n: int = Vec.tally(); };",
            "`Vec` has no associated function `tally`",
        ),
        (
            "an unknown associated function",
            "use std;\nuse std::text;\nuse std::dynamic;\nuse std::process;\nuse std::Option;\nuse std::dynamic::Map;\nuse std::dynamic::Vec;\nrec Point { x: int };\nfn main() { let p: Point = Point.origin(); };",
            "`Point` has no associated function `origin`",
        ),
    ]);
}
/// The audit's rule-conformance probes, order: Resolution is order-free, so a
/// name may be declared after the point that names it and a function may call
/// itself; a cycle among a namespace's module values is the one order error;
/// the method pool admits methods that differ by receiver head, and a type may
/// be recursive.
///
/// Each case is the smallest program for one stated rule.
#[test]
fn rule_conformance_order_and_scope() {
    expect_checked_accepts(&[
        (
            "a local shadows a top-level value",
            "use std;\nuse std::text;\nuse std::dynamic;\nuse std::process;\nuse std::Option;\nuse std::dynamic::Map;\nuse std::dynamic::Vec;\nlet a: int = 1;\nfn main() { let a: int = 2; std::print(\"@(a)\"); };\n",
        ),
        (
            "a method name repeats once per receiver head",
            "use std;\nuse std::text;\nuse std::dynamic;\nuse std::process;\nuse std::Option;\nuse std::dynamic::Map;\nuse std::dynamic::Vec;\nrec A { x: int };\nrec B { y: int };\nimpl A { pub fn about(self: A) -> int { return 1; }; };\nimpl B { pub fn about(self: B) -> int { return 2; }; };\nfn main() { std::print(\"ok\"); };\n",
        ),
        (
            "a method call dispatches on the receiver's type",
            "use std;\nuse std::text;\nuse std::dynamic;\nuse std::process;\nuse std::Option;\nuse std::dynamic::Map;\nuse std::dynamic::Vec;\nfn main() {\n  let xs: Vec<txt> = Vec.new().push(\"a\").push(\"b\").push(\"c\");\n  let n: int = xs.len();\n  let m: int = \"abc\".strlen();\n  std::print(\"@(n) @(m)\");\n};\n",
        ),
        (
            "a method needs no `use` in the file",
            "fn main() {\n  let xs: std::dynamic::Vec<int> = std::dynamic::Vec.new();\n  xs = xs.push(1);\n  std::print(\"@(xs.len())\");\n};\n",
        ),
        (
            "an absolute path needs no `use`",
            "fn main() { let m: std::dynamic::Map<txt, int> = std::dynamic::Map.new(); std::print(\"@(std::dynamic::len(m))\"); };\n",
        ),
        (
            "a recursive declared type",
            "use std;\nuse std::text;\nuse std::dynamic;\nuse std::process;\nuse std::Option;\nuse std::dynamic::Map;\nuse std::dynamic::Vec;\nrec Tree { name: txt, kids: Vec<Tree> };\nfn main() { let t: Tree = { name = \"r\", kids = Vec.new() }; std::print(t.name); };\n",
        ),
        (
            "a forward reference resolves",
            "use std;\nuse std::text;\nuse std::dynamic;\nuse std::process;\nuse std::Option;\nuse std::dynamic::Map;\nuse std::dynamic::Vec;\nfn main() { std::print(banner()); };\nfn banner() -> txt { return \"x\"; };\n",
        ),
        (
            "a function may call itself",
            "use std;\nuse std::text;\nuse std::dynamic;\nuse std::process;\nuse std::Option;\nuse std::dynamic::Map;\nuse std::dynamic::Vec;\nfn f(n: int) -> int { match n { 0 => { return 0; }; _ => {}; }; return f(n - 1); };\nfn main() { std::print(\"@(f(1))\"); };\n",
        ),
        (
            "an associated function and a method may share a name",
            "use std;\nuse std::text;\nuse std::dynamic;\nuse std::process;\nuse std::Option;\nuse std::dynamic::Map;\nuse std::dynamic::Vec;\nrec A { x: int };\nimpl A { pub fn m() -> A { return { x = 1 }; }; pub fn m(self: A) -> int { return self.x; }; };\nfn main() { let a: A = A.m(); std::print(\"@(a.m())\"); };\n",
        ),
        (
            "a `use` is order-free",
            "use std;\nuse std::text;\nuse std::dynamic;\nuse std::process;\nuse std::Option;\nuse std::dynamic::Map;\nuse std::dynamic::Vec;\nuse root;\nfn main() {};\n",
        ),
    ]);
    expect_checked_rejections(&[
        (
            "a value that references itself",
            "use std;\nuse std::text;\nuse std::dynamic;\nuse std::process;\nuse std::Option;\nuse std::dynamic::Map;\nuse std::dynamic::Vec;\nlet x: int = x;\nfn main() { std::print(\"@(x)\"); };\n",
            "`x` is part of a cycle among module values",
        ),
        (
            "a duplicate local",
            "use std;\nuse std::text;\nuse std::dynamic;\nuse std::process;\nuse std::Option;\nuse std::dynamic::Map;\nuse std::dynamic::Vec;\nfn main() { let a: int = 1; let a: int = 2; std::print(\"@(a)\"); };\n",
            "`a` is declared more than once",
        ),
        (
            "a value and a function share a name",
            "use std;\nuse std::text;\nuse std::dynamic;\nuse std::process;\nuse std::Option;\nuse std::dynamic::Map;\nuse std::dynamic::Vec;\nlet thing: txt = \"v\";\nfn thing() -> txt { return \"f\"; };\nfn main() {};\n",
            "`thing` is declared more than once in this namespace",
        ),
        (
            "two methods for one receiver head",
            "use std;\nuse std::text;\nuse std::dynamic;\nuse std::process;\nuse std::Option;\nuse std::dynamic::Map;\nuse std::dynamic::Vec;\nimpl txt { pub fn len(self: txt) -> int { return 1; }; pub fn len(self: txt) -> int { return 2; }; };\nfn main() { std::print(\"a\"); };\n",
            "`len` is declared more than once in this namespace",
        ),
        (
            "a method declared after a head-less function",
            "use std;\nuse std::text;\nuse std::dynamic;\nuse std::process;\nuse std::Option;\nuse std::dynamic::Map;\nuse std::dynamic::Vec;\nrec A { x: int };\nfn m() { std::print(\"x\"); };\nimpl A { pub fn m(self: A) -> int { return 2; }; };\nfn main() {};\n",
            "`m` is declared more than once in this namespace",
        ),
        (
            "a method declared after a module value",
            "use std;\nuse std::text;\nuse std::dynamic;\nuse std::process;\nuse std::Option;\nuse std::dynamic::Map;\nuse std::dynamic::Vec;\nrec A { x: int };\nlet m: int = 1;\nimpl A { pub fn m(self: A) -> int { return 2; }; };\nfn main() {};\n",
            "`m` is declared more than once in this namespace",
        ),
        (
            "a module value declared after a method",
            "use std;\nuse std::text;\nuse std::dynamic;\nuse std::process;\nuse std::Option;\nuse std::dynamic::Map;\nuse std::dynamic::Vec;\nrec A { x: int };\nimpl A { pub fn m(self: A) -> int { return 2; }; };\nlet m: int = 1;\nfn main() {};\n",
            "`m` is declared more than once in this namespace",
        ),
    ]);
}
