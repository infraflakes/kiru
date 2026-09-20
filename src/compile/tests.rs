use crate::ir::{ArmPattern, Node, NodeId, NodeKind, Program, Segment, Template};
use std::sync::atomic::{AtomicU64, Ordering};

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Compile a string of kiru source into a `Program` entirely in memory. All
/// compiler errors are surfaced via `unwrap` so tests fail loudly.
fn compile_str(src: &str) -> Program {
    let name = format!("<test {}>", TEMP_COUNTER.fetch_add(1, Ordering::Relaxed));
    crate::compile::compile_source(&name, src).unwrap_or_else(|e| panic!("compile failed: {:?}", e))
}

/// The run's root nodes.
fn roots<'a>(program: &'a Program, run: &str) -> &'a [NodeId] {
    program
        .runs
        .get(run)
        .unwrap_or_else(|| panic!("run `{run}`"))
}

/// The one child node of `id`.
fn only_child(program: &Program, id: NodeId) -> NodeId {
    let children = &program.node(id).children;
    assert_eq!(children.len(), 1, "expected exactly one child");
    children[0]
}

/// The first child of a run.
fn first_node<'a>(program: &'a Program, run: &str) -> &'a Node {
    let root = roots(program, run)
        .first()
        .copied()
        .expect("run has a node");
    program.node(root)
}

/// The first `Log` or `Exec` template found under a run, in pre-order.
fn first_command_template<'a>(program: &'a Program, run: &str, want_log: bool) -> &'a Template {
    fn find<'a>(program: &'a Program, children: &[NodeId], want_log: bool) -> Option<&'a Template> {
        for &id in children {
            let node = program.node(id);
            match &node.kind {
                NodeKind::Log(t) if want_log => return Some(t),
                NodeKind::Exec(t) if !want_log => return Some(t),
                _ => {}
            }
            if let Some(found) = find(program, &node.children, want_log) {
                return Some(found);
            }
        }
        None
    }
    find(program, roots(program, run), want_log).unwrap_or_else(|| {
        panic!(
            "no {} template in run `{run}`",
            if want_log { "log" } else { "exec" }
        )
    })
}

/// The first `switch` node under a run, in pre-order.
fn first_switch<'a>(program: &'a Program, run: &str) -> &'a Node {
    fn find<'a>(program: &'a Program, children: &[NodeId]) -> Option<&'a Node> {
        for &id in children {
            let node = program.node(id);
            if matches!(node.kind, NodeKind::Switch(_)) {
                return Some(node);
            }
            if let Some(found) = find(program, &node.children) {
                return Some(found);
            }
        }
        None
    }
    find(program, roots(program, run)).unwrap_or_else(|| panic!("no switch in run `{run}`"))
}

/// The documented rows of a run, in display order: label and depth. Mirrors
/// the renderer's walk, without any runtime state.
fn rows(program: &Program, run: &str) -> Vec<(usize, String)> {
    fn walk(program: &Program, children: &[NodeId], depth: usize, out: &mut Vec<(usize, String)>) {
        for &id in children {
            let node = program.node(id);
            match &node.kind {
                NodeKind::Project(_) | NodeKind::Switch(_) => {
                    walk(program, &node.children, depth, out);
                }
                kind => {
                    if let Some(label) = kind.row_label() {
                        out.push((depth, label));
                    }
                    if matches!(kind, NodeKind::Env(_) | NodeKind::Async | NodeKind::Arm(_)) {
                        walk(program, &node.children, depth + 1, out);
                    }
                }
            }
        }
    }
    let mut out = Vec::new();
    walk(program, roots(program, run), 0, &mut out);
    out
}

/// The concatenated text of an IR template: literals verbatim, commands
/// blank (they never ran), variable references expanded to the value they
/// stand for. These tests assert on fully-inlined templates.
fn template_text(template: &Template) -> String {
    template
        .parts
        .iter()
        .map(|part| match part {
            Segment::Lit(text) => text.clone(),
            Segment::Cmd(_) => String::new(),
            Segment::Ref { template, .. } => template_text(template),
        })
        .collect()
}

#[test]
fn test_compile_basic_run() {
    let program = compile_str(
        "\
var channel = (unstable);
fn eval(channel) { log(evaluating @(channel)); };
run bootstrap { project(nix) { eval(@(channel)); }; };
",
    );
    // The run has one node: the project context, whose child is the log.
    let project = first_node(&program, "bootstrap");
    match &project.kind {
        NodeKind::Project(name) => match name.parts.as_slice() {
            [Segment::Lit(name)] => assert_eq!(name, "nix"),
            other => panic!("expected a literal project name, got {:?}", other),
        },
        other => panic!("expected a project context, got {:?}", other),
    }
    let root = roots(&program, "bootstrap")[0];
    let log = only_child(&program, root);
    match &program.node(log).kind {
        NodeKind::Log(t) => assert_eq!(template_text(t), "evaluating unstable"),
        other => panic!("expected log, got {:?}", other),
    }
}

#[test]
fn test_arguments_bind_params() {
    let program = compile_str(
        "\
fn greet(app; suffix) {
    log(hello @(app)@(suffix));
};
run greet_run { project(web) { greet(web-app;!); }; };
",
    );
    let template = first_command_template(&program, "greet_run", true);
    assert_eq!(template_text(template), "hello web-app!");
}

/// A function sees only its parameters and the vars its own body declares:
/// file variables and the caller's local bindings are never implicit.
#[test]
fn test_functions_see_only_their_parameters() {
    let program = compile_str(
        "\
var app = (global-app);
fn show(app) { log(@(app)); };
run shadow { project(p) { show(project-app); }; };
",
    );
    let template = first_command_template(&program, "shadow", true);
    assert_eq!(template_text(template), "project-app");

    // A file variable with the same name is not a fallback either.
    let error = compile_error(
        "\
var app = (global-app);
fn show { log(@(app)); };
run shadow { project(p) { show(); }; };
",
    );
    assert!(error.contains("undefined variable: app"), "{error}");

    let error = compile_error(
        "\
fn inner { log(@(hidden)); };
fn outer {
    var hidden = (H);
    inner();
};
run r { project(p) { outer(); }; };
",
    );
    assert!(error.contains("undefined variable: hidden"), "{error}");
}

/// A run body still resolves file variables and its own local binds.
#[test]
fn test_run_bodies_see_file_variables() {
    let program = compile_str(
        "\
var host = (example.com);
run r {
    var port = (8080);
    log(@(host):@(port));
};",
    );
    let template = first_command_template(&program, "r", true);
    assert_eq!(template_text(template), "example.com:8080");
}

/// A function's own `var` binds need no outer scope at all.
#[test]
fn test_function_local_var_works_without_globals() {
    let program = compile_str(
        "\
fn announce {
    var who = (world);
    log(hello @(who));
};
run r { announce(); };",
    );
    let template = first_command_template(&program, "r", true);
    assert_eq!(template_text(template), "hello world");
}

/// A function call inlines the callee body carbon-copy: statements in
/// order, with the callee's body spliced where the call sits.
#[test]
fn test_calls_inline_carbon_copy() {
    let program = compile_str(
        "\
fn step { log(step-run); };
fn build {
    log(before);
    step();
    log(after);
};
run ci { build(); };
",
    );
    let labels: Vec<String> = rows(&program, "ci")
        .into_iter()
        .map(|(_, label)| label)
        .collect();
    assert_eq!(labels, vec!["log: before", "log: step-run", "log: after"]);
}

#[test]
fn test_compile_switch_lowering() {
    let program = compile_str(
        "\
var os = (linux);
fn pick(os) {
    switch(@(os)) {
        case(linux) { log(linux-path); };
        default { log(other); };
    };
};
run pick_run { project(p) { pick(@(os)); }; };
",
    );
    let switch = first_switch(&program, "pick_run");
    assert_eq!(switch.children.len(), 2);
    let second = program.node(switch.children[1]);
    assert!(matches!(second.kind, NodeKind::Arm(ArmPattern::Default)));
    // The taken arm owns its body; the switch itself has no row.
    let first = program.node(switch.children[0]);
    assert_eq!(first.children.len(), 1);
    assert!(matches!(
        program.node(first.children[0]).kind,
        NodeKind::Log(_)
    ));
}

/// The run layout mirrors the display rows: a sequential top-level call is
/// its own row; an async block indents its steps under one label.
#[test]
fn test_run_layout_expands_calls_and_indents_async() {
    let program = compile_str(
        "\
fn a { log(a); };
fn b { log(b); };
run d {
    a();
    async() {
        b();
        a();
    };
};
",
    );
    assert_eq!(
        rows(&program, "d"),
        vec![
            (0, "log: a".to_string()),
            (0, "async".to_string()),
            (1, "log: b".to_string()),
            (1, "log: a".to_string()),
        ]
    );
}

/// The full layout of nested async/project/call bodies, asserted row by row.
#[test]
fn test_layout_rows_cover_every_statement_in_order() {
    let program = compile_str(
        "\
fn fmt_step { log(a); exec(b); exec(c); };
fn test_kiru { log(t); exec(d); };
fn clean_kiru { log(cl); };
fn build_kiru { log(b); };
run test_kiru {
    async() {
        project(kiru) { log(x); exec(y); };
        test_kiru();
    };
    log(main);
    async() { clean_kiru(); build_kiru(); };
};
",
    );
    assert_eq!(
        rows(&program, "test_kiru"),
        vec![
            (0, "async".to_string()),
            (1, "log: x".to_string()),
            (1, "exec: y".to_string()),
            (1, "log: t".to_string()),
            (1, "exec: d".to_string()),
            (0, "log: main".to_string()),
            (0, "async".to_string()),
            (1, "log: cl".to_string()),
            (1, "log: b".to_string()),
        ]
    );
}

/// `case(@(x))` is legal: the reference inlines to a literal at compile time.
#[test]
fn test_case_pattern_inlines_variable_references() {
    let program = compile_str(
        "\
var target = (prod);
fn pick(target) {
    switch(@(target)) {
        case(@(target)) { log(matched); };
        default { log(other); };
    };
};
run r { pick(@(target)); };
",
    );
    let switch = first_switch(&program, "r");
    let first = program.node(switch.children[0]);
    assert!(
        matches!(&first.kind, NodeKind::Arm(ArmPattern::Lit(p)) if p == "prod"),
        "got {:?}",
        first.kind
    );
}

/// `$(cmd)` in a pattern has no static text and stays rejected at compile.
#[test]
fn test_case_command_pattern_is_rejected() {
    // The function must be called: lowering is per call site, so a dead
    // function is never compiled.
    let error =
        compile_error("fn d { switch(v) { case($(cmd)) { log(x); }; }; };\nrun r { d(); };");
    assert!(
        error.contains("case pattern must be literal text"),
        "{error}"
    );
}

/// `exec` resolves substitutions, then runs the resulting text.
#[test]
fn test_exec_inlines_variables() {
    let program = compile_str(
        "\
var name = (kiru);
fn build(name) { exec(echo @(name)); };
run r { build(@(name)); };
",
    );
    let template = first_command_template(&program, "r", false);
    assert_eq!(template_text(template), "echo kiru");
}

#[test]
fn test_undefined_function_call_is_rejected() {
    let error = compile_error("run r { project(p) { nonexistent(); }; };");
    assert!(
        error.contains("undefined function: `nonexistent`"),
        "{error}"
    );
}

/// A redefinition is textual: earlier mentions keep the definition visible
/// at their own declaration point, later ones see the new one.
#[test]
fn test_function_redefinition_is_textual() {
    let program = compile_str(
        "\
fn f { log(first); };
run before { f(); };
fn f { log(second); };
run after { f(); };
",
    );
    assert_eq!(
        template_text(first_command_template(&program, "before", true)),
        "first"
    );
    assert_eq!(
        template_text(first_command_template(&program, "after", true)),
        "second"
    );
}

#[test]
fn test_argument_count_is_checked() {
    let error = compile_error(
        "\
fn deploy(name) { log(@(name)); };
run r { project(p) { deploy(a;b); }; };
",
    );
    assert!(
        error.contains("function `deploy` takes 1 argument(s), got 2"),
        "{error}"
    );
}

#[test]
fn test_recursive_function_is_rejected() {
    let error = compile_error(
        "\
fn step { step(); };
run r { project(p) { step(); }; };
",
    );
    assert!(
        error.contains("circular function call: step -> step"),
        "{error}"
    );
}

/// Mutual recursion cannot even be written: the mention of `b` inside `a`
/// comes before `b` is declared.
#[test]
fn test_mutual_recursion_fails_by_order() {
    let error = compile_error(
        "\
fn a { b(); };
fn b { a(); };
run r { project(p) { a(); }; };
",
    );
    assert!(error.contains("undefined function: `b`"), "{error}");
}

/// Everything is read top-down: a function mentioned before its declaration
/// is not visible.
#[test]
fn test_functions_must_be_declared_before_use() {
    let error = compile_error("run r { project(p) { step(); }; };\nfn step { log(step-run); };");
    assert!(error.contains("undefined function: `step`"), "{error}");
}

/// Validation is not lazy: a dead function body that mentions a later
/// declaration or an undefined name still errors.
#[test]
fn test_dead_function_bodies_are_validated() {
    let error = compile_error("fn a { b(); };\nfn b { log(x); };\nrun r { log(ok); };");
    assert!(error.contains("undefined function: `b`"), "{error}");

    let error = compile_error("fn dead { log(@(missing)); };\nrun r { log(ok); };");
    assert!(error.contains("undefined variable: missing"), "{error}");
}

/// Imports join the namespace at the point they appear: a use before the
/// import cannot see its definitions, a use after can.
#[test]
fn test_imports_are_visible_from_their_point_on() {
    let dir = tempfile::tempdir().unwrap();
    let helper = dir.path().join("shared.kiru");
    std::fs::write(&helper, "fn step { log(step-run); };").unwrap();

    let before = format!("run r {{ step(); }};\nimport({});\n", helper.display());
    let error = compile_error(&before);
    assert!(error.contains("undefined function: `step`"), "{error}");

    let after = format!("import({});\nrun r {{ step(); }};\n", helper.display());
    let program = compile_str(&after);
    assert_eq!(
        template_text(first_command_template(&program, "r", true)),
        "step-run"
    );
}

/// A run sees the variables declared before it, never a later one.
#[test]
fn test_runs_see_only_earlier_variables() {
    let error = compile_error("run r { log(@(later)); };\nvar later = (L);");
    assert!(error.contains("undefined variable: later"), "{error}");

    let program = compile_str("var earlier = (E);\nrun r { log(@(earlier)); };");
    assert_eq!(
        template_text(first_command_template(&program, "r", true)),
        "E"
    );
}

/// A variable redefinition wins from its point on; earlier runs keep the
/// value that was visible when they were declared.
#[test]
fn test_variable_redefinition_wins_from_that_point() {
    let program = compile_str(
        "\
var x = (one);
run first { log(@(x)); };
var x = (two);
run second { log(@(x)); };
",
    );
    assert_eq!(
        template_text(first_command_template(&program, "first", true)),
        "one"
    );
    assert_eq!(
        template_text(first_command_template(&program, "second", true)),
        "two"
    );
}

/// A variable reference is a tagged value, not a spliced copy: two uses in
/// one template share one identity so the runtime computes it once.
#[test]
fn test_variable_reference_is_tagged_and_shared() {
    let program = compile_str(
        "\
var x = ($(echo hi));
run r { log(@(x)@(x)); };
",
    );
    let template = first_command_template(&program, "r", true);
    let refs: Vec<(usize, &Template)> = template
        .parts
        .iter()
        .filter_map(|part| match part {
            Segment::Ref { id, template } => Some((*id, template)),
            _ => None,
        })
        .collect();
    assert_eq!(refs.len(), 2, "both uses are tagged: {template:?}");
    assert_eq!(refs[0].0, refs[1].0, "both uses share one identity");
    assert!(
        refs[0]
            .1
            .parts
            .iter()
            .any(|part| matches!(part, Segment::Cmd(_))),
        "the tagged value carries the command to run once"
    );
}

/// A literal variable is still acceptable as a case pattern.
#[test]
fn test_case_pattern_with_literal_variable() {
    let program = compile_str(
        "\
var target = (prod);
run r {
    switch(@(target)) {
        case(@(target)) { log(matched); };
        default { log(other); };
    };
};
",
    );
    let switch = first_switch(&program, "r");
    let first = program.node(switch.children[0]);
    assert!(
        matches!(&first.kind, NodeKind::Arm(ArmPattern::Lit(p)) if p == "prod"),
        "got {:?}",
        first.kind
    );
}

/// A bare `$()`/`@()` is not a value: it must be wrapped in `(...)`.
#[test]
fn test_bare_value_must_be_wrapped() {
    let error = compile_error("var x = $(cmd);");
    assert!(error.contains("is only valid inside"), "{error}");
}

/// A variable holding a command cannot be a case pattern: the pattern must
/// be concrete text.
#[test]
fn test_case_pattern_with_command_variable_is_rejected() {
    let error =
        compile_error("var t = ($(echo x));\nrun r { switch(y) { case(@(t)) { log(z); }; }; };");
    assert!(
        error.contains("case pattern must be literal text"),
        "{error}"
    );
}

/// Duplicate parameters bind the last argument.
#[test]
fn test_last_parameter_wins() {
    let program = compile_str(
        "\
fn f(a; a) { log(@(a)); };
run r { f(one;two); };
",
    );
    assert_eq!(
        template_text(first_command_template(&program, "r", true)),
        "two"
    );
}

/// An unqualified run call runs at the invocation context: no project
/// wrapper, just the inlined node.
#[test]
fn test_unqualified_run_call_has_no_context() {
    let program = compile_str("fn build { log(b); };\n run r { build(); };");
    assert!(
        !matches!(first_node(&program, "r").kind, NodeKind::Project(_)),
        "unqualified call must not switch project context"
    );
}

#[test]
fn test_compile_unknown_run_reference_fails() {
    let file = std::env::temp_dir().join(format!("kiru_test_err_{}.kiru", std::process::id()));
    std::fs::write(
        &file,
        "fn eval { log(x); }; run bad { project(nix) { missing(); }; };",
    )
    .unwrap();
    let result = crate::compile::compile_path(&file);
    let _ = std::fs::remove_file(&file);
    assert!(result.is_err());
}

/// A function from an imported file reports diagnostics against that file.
#[test]
fn test_imported_function_reports_against_its_own_source() {
    let dir = tempfile::tempdir().unwrap();
    let helper = dir.path().join("shared.kiru");
    std::fs::write(&helper, "fn step { log(@(missing_var)); };").unwrap();
    let entry = dir.path().join("main.kiru");
    std::fs::write(
        &entry,
        format!(
            "import({});\nrun r {{ project(p) {{ step(); }}; }};\n",
            helper.display()
        ),
    )
    .unwrap();
    let error = match crate::compile::compile_path(&entry) {
        Err(crate::compile::CompileError::Diagnostics(diags)) => diags[0].clone(),
        other => panic!("expected diagnostics, got {:?}", other),
    };
    assert_eq!(error.file, helper.display().to_string(), "{:?}", error);
    assert!(
        error.message.contains("undefined variable: missing_var"),
        "{:?}",
        error
    );
}

fn compile_error(src: &str) -> String {
    match crate::compile::compile_source("<test error>", src) {
        Err(crate::compile::CompileError::Diagnostics(diags)) => diags
            .iter()
            .map(|d| d.message.clone())
            .collect::<Vec<_>>()
            .join("; "),
        other => panic!("expected diagnostics, got {:?}", other),
    }
}
