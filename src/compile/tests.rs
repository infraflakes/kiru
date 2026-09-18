use crate::ir::{Instruction, Ir};

/// The instruction body of the display row `row` in a run, searched through
/// the run body, its async groups, project contexts, and switch arms.
fn task_body<'a>(ir: &'a Ir, run: &str, row: usize) -> &'a [Instruction] {
    fn find<'a>(body: &'a [Instruction], row: usize) -> Option<&'a [Instruction]> {
        for instruction in body {
            match instruction {
                Instruction::Step { row: r, body, .. } => {
                    if *r == row {
                        return Some(body);
                    }
                    if let Some(found) = find(body, row) {
                        return Some(found);
                    }
                }
                Instruction::Switch { arms, .. } => {
                    for arm in arms {
                        if arm.row == row {
                            return Some(&arm.body);
                        }
                        if let Some(found) = find(&arm.body, row) {
                            return Some(found);
                        }
                    }
                }
                Instruction::Async { body } | Instruction::Context { body, .. } => {
                    if let Some(found) = find(body, row) {
                        return Some(found);
                    }
                }
                _ => {}
            }
        }
        None
    }
    find(
        ir.runs.get(run).unwrap_or_else(|| panic!("run `{run}`")),
        row,
    )
    .unwrap_or_else(|| panic!("row {row} in run `{run}`"))
}

/// The first switch in a run tree, with its arms.
fn find_switch_arms<'a>(ir: &'a Ir, run: &str) -> Option<&'a [crate::ir::Arm]> {
    fn find(body: &[Instruction]) -> Option<&[crate::ir::Arm]> {
        for instruction in body {
            match instruction {
                Instruction::Switch { arms, .. } => return Some(arms),
                Instruction::Step { body, .. }
                | Instruction::Async { body }
                | Instruction::Context { body, .. } => {
                    if let Some(found) = find(body) {
                        return Some(found);
                    }
                }
                _ => {}
            }
        }
        None
    }
    find(&ir.runs[run])
}

#[test]
fn test_compile_basic_run() {
    let ir = crate::compile::test_support::compile_str(
        "\
var channel = (unstable);
fn eval { log(evaluating @(channel)); };
run bootstrap { project(nix) { eval(); }; };
",
    );
    // The project block wraps the row: `run bootstrap { project(nix) { eval(); }; };`
    match ir.runs["bootstrap"].first() {
        Some(Instruction::Context { project, body }) => {
            match project.parts.as_slice() {
                [crate::ir::Segment::Lit(name)] => assert_eq!(name, "nix"),
                other => panic!("expected a literal project name, got {:?}", other),
            }
            assert!(matches!(
                body.first(),
                Some(Instruction::Step { row: 0, .. })
            ));
        }
        other => panic!("expected context, got {:?}", other),
    }
    match task_body(&ir, "bootstrap", 0).first() {
        Some(Instruction::Log(t)) => assert_eq!(template_text(t), "evaluating unstable"),
        other => panic!("expected log, got {:?}", other),
    }
}

#[test]
fn test_arguments_bind_params() {
    let ir = crate::compile::test_support::compile_str(
        "\
fn greet(app; suffix) {
    log(hello @(app)@(suffix));
};
run greet_run { project(web) { greet(web-app; !); }; };
",
    );
    match task_body(&ir, "greet_run", 0).first() {
        Some(Instruction::Log(t)) => assert_eq!(template_text(t), "hello web-app!"),
        other => panic!("expected log, got {:?}", other),
    }
}

/// Params shadow global variables, and the callee sees only params plus
/// globals - never the caller's local bindings.
#[test]
fn test_callee_scope_is_params_then_globals() {
    let ir = crate::compile::test_support::compile_str(
        "\
var app = (global-app);
fn show(app) { log(@(app)); };
run shadow { project(p) { show(project-app); }; };
",
    );
    match task_body(&ir, "shadow", 0).first() {
        Some(Instruction::Log(t)) => assert_eq!(template_text(t), "project-app"),
        other => panic!("expected log, got {:?}", other),
    }

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
    let ir = crate::compile::test_support::compile_str(
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
    // Calls flatten: the callee's step becomes a sibling row.
    let rows: Vec<String> = ir
        .run_plan("ci")
        .into_iter()
        .map(|line| line.label)
        .collect();
    assert_eq!(rows, vec!["log before", "log step-run", "log after"]);
    for (row, expected) in [(0, "before"), (1, "step-run"), (2, "after")] {
        match task_body(&ir, "ci", row).first() {
            Some(Instruction::Log(t)) => assert_eq!(template_text(t), expected),
            other => panic!("expected log, got {:?}", other),
        }
    }
}

#[test]
fn test_compile_switch_lowering() {
    let ir = crate::compile::test_support::compile_str(
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
    let arms = find_switch_arms(&ir, "pick_run").expect("switch in pick_run");
    assert_eq!(arms.len(), 2);
    assert!(matches!(arms[1].pattern, crate::ir::ArmPattern::Default));
    // Arm rows are interleaved with their bodies: arm 0, its step, arm 1.
    assert_eq!(arms[0].row, 0);
    assert_eq!(task_body(&ir, "pick_run", 0).len(), 1);
    assert_eq!(arms[1].row, 2);
}

/// The run layout mirrors the display rows: a sequential top-level call is
/// its own group; an async block groups its steps under one label.
#[test]
fn test_run_plan_expands_calls_and_indents_async() {
    let ir = crate::compile::test_support::compile_str(
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
    let plan: Vec<(usize, String)> = ir
        .run_plan("d")
        .into_iter()
        .map(|line| (line.depth, line.label))
        .collect();
    assert_eq!(
        plan,
        vec![
            (0, "log a".to_string()),
            (0, "async".to_string()),
            (1, "log b".to_string()),
            (1, "log a".to_string()),
        ]
    );
    // Every planned row exists in the instruction tree.
    assert_eq!(task_body(&ir, "d", 0).len(), 1);
    assert_eq!(task_body(&ir, "d", 2).len(), 1);
    assert_eq!(task_body(&ir, "d", 3).len(), 1);
}

/// `case(@(x))` is legal: the reference inlines to a literal at compile time.
#[test]
fn test_case_pattern_inlines_variable_references() {
    let ir = crate::compile::test_support::compile_str(
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
    let arms = find_switch_arms(&ir, "r").expect("switch in r");
    assert!(
        matches!(&arms[0].pattern, crate::ir::ArmPattern::Lit(p) if p == "prod"),
        "got {:?}",
        arms[0].pattern
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
    let ir = crate::compile::test_support::compile_str(
        "\
var name = (kiru);
fn build { exec(echo @(name)); };
run r { build(); };
",
    );
    match &task_body(&ir, "r", 0)[0] {
        Instruction::Exec { command } => assert_eq!(template_text(command), "echo kiru"),
        other => panic!("expected exec, got {:?}", other),
    }
}

/// The concatenated literal text of an IR template. These tests assert on
/// fully-inlined templates, so the literal text is their whole value.
fn template_text(template: &crate::ir::Template) -> String {
    template
        .parts
        .iter()
        .map(|part| match part {
            crate::ir::Segment::Lit(text) => text.clone(),
            crate::ir::Segment::Cmd(_) => String::new(),
        })
        .collect()
}

/// Every display row in a run, ordered by its compile-assigned index.
fn rows_by_index(ir: &Ir, run: &str) -> Vec<(usize, String)> {
    fn walk(body: &[Instruction], found: &mut Vec<(usize, String)>) {
        for instruction in body {
            match instruction {
                Instruction::Step { row, label, body } => {
                    found.push((*row, label.clone()));
                    walk(body, found);
                }
                Instruction::Switch { arms, .. } => {
                    for arm in arms {
                        let label = match &arm.pattern {
                            crate::ir::ArmPattern::Lit(pattern) => {
                                format!("switch case {pattern}")
                            }
                            crate::ir::ArmPattern::Default => "switch default".to_string(),
                        };
                        found.push((arm.row, label));
                        walk(&arm.body, found);
                    }
                }
                Instruction::Async { body } | Instruction::Context { body, .. } => {
                    walk(body, found);
                }
                _ => {}
            }
        }
    }
    let mut found = Vec::new();
    walk(&ir.runs[run], &mut found);
    found.sort_by_key(|(row, _)| *row);
    found
}

/// The layout's flat rows must equal the `Task` labels ordered by their
/// compile-assigned row index: projects inside async groups, main-thread
/// statements, and nested calls all included, in order. Any drift between
/// lowering and the display projection breaks output routing and can end
/// the run early.
#[test]
fn test_layout_rows_match_compile_assigned_indices() {
    let ir = crate::compile::test_support::compile_str(
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
    let plan_rows: Vec<(usize, String)> = ir
        .run_plan("test_kiru")
        .into_iter()
        .map(|line| (line.row, line.label))
        .collect();
    assert_eq!(
        plan_rows,
        rows_by_index(&ir, "test_kiru"),
        "the display plan must match the compile-assigned row order"
    );
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
    let ir = crate::compile::test_support::compile_str(
        "\
run r { project(p) { step(); }; };
fn step { log(step-run); };
",
    );
    match task_body(&ir, "r", 0).first() {
        Some(Instruction::Log(t)) => assert_eq!(template_text(t), "step-run"),
        other => panic!("expected log, got {:?}", other),
    }
}

/// An unqualified run call runs at the invocation context: no `Context`
/// wrapper, just the inlined body.
#[test]
fn test_unqualified_run_call_has_no_context() {
    let ir =
        crate::compile::test_support::compile_str("fn build { log(b); };\n run r { build(); };");
    let body = task_body(&ir, "r", 0);
    assert!(
        !matches!(body.first(), Some(Instruction::Context { .. })),
        "unqualified call must not switch project context: {body:?}"
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

fn count_instructions(body: &[Instruction]) -> Vec<&'static str> {
    body.iter()
        .map(|i| match i {
            Instruction::Log(_) => "log",
            Instruction::Exec { .. } => "exec",
            Instruction::Cd(_) => "cd",
            Instruction::Env { .. } => "env",
            Instruction::Switch { .. } => "switch",
            Instruction::Step { .. } => "step",
            Instruction::Async { .. } => "async",
            Instruction::Context { .. } => "context",
        })
        .collect()
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
