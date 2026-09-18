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

/// The concatenated literal text of an IR template. These tests assert on
/// fully-inlined templates, so the literal text is their whole value.
fn template_text(template: &Template) -> String {
    template
        .parts
        .iter()
        .map(|part| match part {
            Segment::Lit(text) => text.clone(),
            Segment::Cmd(_) => String::new(),
        })
        .collect()
}

#[test]
fn test_compile_basic_run() {
    let program = compile_str(
        "\
var channel = (unstable);
fn eval { log(evaluating @(channel)); };
run bootstrap { project(nix) { eval(); }; };
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
run greet_run { project(web) { greet(web-app; !); }; };
",
    );
    let template = first_command_template(&program, "greet_run", true);
    assert_eq!(template_text(template), "hello web-app!");
}

/// Params shadow global variables, and the callee sees only params plus
/// globals - never the caller's local bindings.
#[test]
fn test_callee_scope_is_params_then_globals() {
    let program = compile_str(
        "\
var app = (global-app);
fn show(app) { log(@(app)); };
run shadow { project(p) { show(project-app); }; };
",
    );
    let template = first_command_template(&program, "shadow", true);
    assert_eq!(template_text(template), "project-app");

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
fn pick {
    switch(@(os)) {
        case(linux) { log(linux-path); };
        default { log(other); };
    };
};
run pick_run { project(p) { pick(); }; };
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
fn pick {
    switch(@(target)) {
        case(@(target)) { log(matched); };
        default { log(other); };
    };
};
run r { pick(); };
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
fn build { exec(echo @(name)); };
run r { build(); };
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

#[test]
fn test_duplicate_function_is_rejected() {
    let error = compile_error(
        "\
fn step { log(one); };
fn step { log(two); };
",
    );
    assert!(error.contains("duplicate function `step`"), "{error}");
}

#[test]
fn test_argument_count_is_checked() {
    let error = compile_error(
        "\
fn deploy(name) { log(@(name)); };
run r { project(p) { deploy(a; b); }; };
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

#[test]
fn test_mutually_recursive_fns_are_rejected() {
    let error = compile_error(
        "\
fn a { b(); };
fn b { a(); };
run r { project(p) { a(); }; };
",
    );
    assert!(
        error.contains("circular function call: a -> b -> a"),
        "{error}"
    );
}

/// Functions are collected in a pre-pass, so a run may call a function
/// declared after it.
#[test]
fn test_function_order_independence() {
    let program = compile_str(
        "\
run r { project(p) { step(); }; };
fn step { log(step-run); };
",
    );
    let template = first_command_template(&program, "r", true);
    assert_eq!(template_text(template), "step-run");
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
