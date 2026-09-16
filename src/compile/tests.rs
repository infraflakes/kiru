#[test]
fn test_compile_basic_project() {
    let ir = crate::compile::test_support::compile_str(
        "\
var home_dir = $(echo /home/user);
project nix {
    var channel = (unstable);
    fn eval { log(evaluating @(channel)); };
};
run bootstrap { nix::eval; };
",
    );
    use crate::ir::Instruction;
    use crate::ir::serialize::write_template;
    assert_eq!(ir.projects.len(), 1);
    let nix = ir.projects.get("nix").expect("nix project");
    // Function-local `var channel` is fully inlined: `@(channel)` -> `unstable`,
    // and no `vars` map survives on the project.
    let eval_body = nix.functions.get("eval").expect("eval fn");
    assert_eq!(eval_body.len(), 1);
    match &eval_body[0] {
        Instruction::Log(t) => {
            assert_eq!(
                write_template(t),
                "(t (lit \"evaluating \") (lit \"unstable\"))"
            )
        }
        _ => panic!("expected log"),
    }
    let calls = ir.execution_chains.get("bootstrap").expect("bootstrap run");
    assert_eq!(calls.len(), 1, "single stage");
    assert_eq!(calls[0].len(), 1, "single call in stage");
    assert_eq!(calls[0][0].fqn(), "nix::eval");
}

#[test]
fn test_compile_unknown_run_reference_fails() {
    let file = std::env::temp_dir().join(format!("kiru_test_err_{}.kiru", std::process::id()));
    std::fs::write(
        &file,
        "project nix { fn eval { log(x); }; } run bad { nix::missing; };",
    )
    .unwrap();
    let result = crate::compile::compile_path(&file);
    let _ = std::fs::remove_file(&file);
    assert!(result.is_err());
}

#[test]
fn test_compile_switch_lowering() {
    let ir = crate::compile::test_support::compile_str(
        "\
project p {
    var os = (linux);
    fn pick {
        switch(@(os)) {
            case(linux) { log(linux-path); };
            default { log(other); };
        };
    };
};
",
    );
    let p = ir.projects.get("p").expect("p project");
    let body = p.functions.get("pick").expect("pick fn");
    assert_eq!(body.len(), 1);
    match &body[0] {
        crate::ir::Instruction::Switch { arms, .. } => {
            assert_eq!(arms.len(), 2);
            assert!(matches!(arms[1].pattern, crate::ir::ArmPattern::Default));
        }
        _ => panic!("expected switch"),
    }
}

/// A global function lowered into a project body must produce exactly the IR
/// of the same statements written inline: the carbon-copy rule.
#[test]
fn test_global_fn_is_a_carbon_copy_of_its_body() {
    let global_form = crate::compile::test_support::compile_str(
        "\
fn step {
    log(stepping);
    $(echo step);
};

project p {
    fn build { step(); };
};
",
    );
    let pasted_form = crate::compile::test_support::compile_str(
        "\
project p {
    fn build {
        log(stepping);
        $(echo step);
    };
};
",
    );
    let inlined = global_form
        .projects
        .get("p")
        .expect("p project")
        .functions
        .get("build")
        .expect("build fn");
    let written = pasted_form
        .projects
        .get("p")
        .expect("p project")
        .functions
        .get("build")
        .expect("build fn");
    assert_eq!(
        format!("{:?}", inlined),
        format!("{:?}", written),
        "a call must lower to exactly the inlined body"
    );
}

/// `@(var)` inside a global function resolves against the including
/// project's vars first (project shadows global), so the same template
/// expands differently per project.
#[test]
fn test_global_fn_resolves_vars_from_the_including_project() {
    let ir = crate::compile::test_support::compile_str(
        "\
var app = (global-default);

fn greet { log(hello @(app)); };

project alpha {
    var app = (alpha-app);
    fn show { greet(); };
};

project beta {
    fn show { greet(); };
};
",
    );
    let alpha_log = global_first_instruction(&ir, "alpha", "show");
    let beta_log = global_first_instruction(&ir, "beta", "show");
    match alpha_log {
        crate::ir::Instruction::Log(t) => assert_eq!(
            crate::ir::serialize::write_template(&t),
            "(t (lit \"hello \") (lit \"alpha-app\"))"
        ),
        other => panic!("expected log, got {:?}", other),
    }
    match beta_log {
        crate::ir::Instruction::Log(t) => assert_eq!(
            crate::ir::serialize::write_template(&t),
            "(t (lit \"hello \") (lit \"global-default\"))"
        ),
        other => panic!("expected log, got {:?}", other),
    }
}

/// A sibling function of the same project resolves before any global
/// function of the same name (project-first, mirroring var shadowing).
#[test]
fn test_sibling_fn_shadows_a_global_fn() {
    let ir = crate::compile::test_support::compile_str(
        "\
fn greet { log(global-greet); };

project p {
    fn greet { log(project-greet); };
    fn main { greet(); };
};
",
    );
    match global_first_instruction(&ir, "p", "main") {
        crate::ir::Instruction::Log(t) => assert_eq!(
            crate::ir::serialize::write_template(&t),
            "(t (lit \"project-greet\"))"
        ),
        other => panic!("expected log, got {:?}", other),
    }
}

#[test]
fn test_undefined_function_call_is_rejected() {
    let error = compile_error(
        "\
project p {
    fn build { nonexistent(); };
};
",
    );
    assert!(
        error
            .contains("undefined function: `nonexistent` (not a function of project `p`, and not a global function)"),
        "{error}"
    );
}

#[test]
fn test_duplicate_global_fn_is_rejected() {
    let error = compile_error(
        "\
fn step { log(one); };
fn step { log(two); };
",
    );
    assert!(
        error.contains("duplicate global function `step`"),
        "{error}"
    );
}

#[test]
fn test_recursive_global_fn_is_rejected() {
    let error = compile_error(
        "\
fn step { step(); };

project p {
    fn build { step(); };
};
",
    );
    assert!(
        error.contains("circular function call: build -> step -> step"),
        "{error}"
    );
}

#[test]
fn test_mutually_recursive_fns_are_rejected() {
    let error = compile_error(
        "\
fn a { b(); };
fn b { a(); };

project p {
    fn build { a(); };
};
",
    );
    assert!(
        error.contains("circular function call: build -> a -> b -> a"),
        "{error}"
    );
}

#[test]
fn test_sibling_fn_cycle_is_rejected() {
    let error = compile_error(
        "\
project p {
    fn a { b(); };
    fn b { a(); };
};
",
    );
    assert!(
        error.contains("circular function call: a -> b -> a"),
        "{error}"
    );
}

/// A global function template called at several points in one body is
/// expanded at each call site, in order.
#[test]
fn test_global_fn_expands_at_every_call_site() {
    let ir = crate::compile::test_support::compile_str(
        "\
fn step { log(step-run); };

project p {
    fn build {
        step();
        log(middle);
        step();
    };
};
",
    );
    let body = ir
        .projects
        .get("p")
        .expect("p")
        .functions
        .get("build")
        .expect("build");
    assert_eq!(count_instructions(&body), vec!["log", "log", "log"]);
    match &body[0] {
        crate::ir::Instruction::Log(t) => assert_eq!(
            crate::ir::serialize::write_template(t),
            "(t (lit \"step-run\"))"
        ),
        other => panic!("expected inlined step log, got {:?}", other),
    }
    match &body[1] {
        crate::ir::Instruction::Log(t) => assert_eq!(
            crate::ir::serialize::write_template(t),
            "(t (lit \"middle\"))"
        ),
        other => panic!("expected the middle log, got {:?}", other),
    }
    match &body[2] {
        crate::ir::Instruction::Log(t) => assert_eq!(
            crate::ir::serialize::write_template(t),
            "(t (lit \"step-run\"))"
        ),
        other => panic!("expected the second inlined step log, got {:?}", other),
    }
}

/// Non-empty call parens are rejected: calls take no arguments.
#[test]
fn test_call_with_arguments_is_rejected() {
    let error = compile_error("project p { fn build { step(hello); }; };");
    assert!(
        error.contains("function call `step` takes no arguments"),
        "{error}"
    );
}

/// A global function from an imported file reports diagnostics against the
/// imported source.
#[test]
fn test_global_fn_from_import_reports_against_its_own_source() {
    let dir = tempfile::tempdir().unwrap();
    let helper = dir.path().join("shared.kiru");
    std::fs::write(&helper, "fn step { log(@(missing_var)); };").unwrap();
    let entry = dir.path().join("main.kiru");
    std::fs::write(
        &entry,
        format!(
            "import({});\nproject p {{ fn build {{ step(); }}; }};\n",
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

/// Helpers shared by the global-function tests.

fn global_first_instruction(
    ir: &crate::ir::Ir,
    project: &str,
    function: &str,
) -> crate::ir::Instruction {
    ir.projects
        .get(project)
        .and_then(|p| p.functions.get(function))
        .and_then(|body| body.first().cloned())
        .unwrap_or_else(|| panic!("expected instruction in {project}::{function}"))
}

fn count_instructions(body: &[crate::ir::Instruction]) -> Vec<&'static str> {
    body.iter()
        .map(|i| match i {
            crate::ir::Instruction::Log(_) => "log",
            crate::ir::Instruction::RunShellCmd { .. } => "run_shell_cmd",
            crate::ir::Instruction::Cd(_) => "cd",
            crate::ir::Instruction::Env { .. } => "env",
            crate::ir::Instruction::Switch { .. } => "switch",
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

/// Global functions are collected in a pre-pass, so a project declared
/// before the `fn` it calls still compiles.
#[test]
fn test_global_fn_order_independence() {
    let ir = crate::compile::test_support::compile_str(
        "\
project p {
    fn build { step(); };
};

fn step { log(step-run); };
",
    );
    match global_first_instruction(&ir, "p", "build") {
        crate::ir::Instruction::Log(t) => assert_eq!(
            crate::ir::serialize::write_template(&t),
            "(t (lit \"step-run\"))"
        ),
        other => panic!("expected log, got {:?}", other),
    }
}

/// A project-body call binds the global function template into the project
/// as a function of the same name, resolved against the project's vars, so
/// run blocks reach it as `project::name`.
#[test]
fn test_project_body_call_materializes_the_global_fn() {
    let ir = crate::compile::test_support::compile_str(
        "\
var name = (global);

fn ssh { log(deploying @(name)); };

project deploy {
    var name = (deploy);
    ssh();
};

run deploy { deploy::ssh; };
",
    );
    let deploy = ir.projects.get("deploy").expect("deploy project");
    let ssh_body = deploy.functions.get("ssh").expect("bound ssh fn");
    assert_eq!(ssh_body.len(), 1);
    match &ssh_body[0] {
        crate::ir::Instruction::Log(t) => assert_eq!(
            crate::ir::serialize::write_template(t),
            "(t (lit \"deploying \") (lit \"deploy\"))"
        ),
        other => panic!("expected log, got {:?}", other),
    }
    // The run block validates against the bound function.
    assert_eq!(
        ir.execution_chains.get("deploy").expect("deploy run")[0][0].fqn(),
        "deploy::ssh"
    );
}

/// A project-body call to a name that is already a project function is a
/// duplicate, whether the fn is declared before or after the call.
#[test]
fn test_project_body_call_duplicate_is_rejected() {
    let error = compile_error(
        "\
fn ssh { log(one); };

project p {
    ssh();
    fn ssh { log(two); };
};
",
    );
    assert!(
        error.contains("duplicate function `ssh` in project `p`"),
        "{error}"
    );

    let error = compile_error("project p { fn ssh { log(two); }; ssh(); };");
    assert!(
        error.contains("duplicate function `ssh` in project `p`"),
        "{error}"
    );
}

#[test]
fn test_project_body_call_unknown_global_is_rejected() {
    let error = compile_error("project p { nonexistent(); };");
    assert!(
        error.contains("undefined function: `nonexistent` (not a global function)"),
        "{error}"
    );
}

/// A bound global template is callable from the project's other functions,
/// and cycles through the bound name are detected.
#[test]
fn test_bound_global_fn_is_callable_and_cycle_checked() {
    let ir = crate::compile::test_support::compile_str(
        "\
fn step { log(step-run); };

project p {
    step();
    fn build {
        step();
        log(main);
    };
};
",
    );
    let body = ir
        .projects
        .get("p")
        .expect("p")
        .functions
        .get("build")
        .expect("build");
    assert_eq!(
        count_instructions(&body),
        vec!["log", "log"],
        "sibling call resolves to the bound template, expanded carbon-copy"
    );
    match &body[0] {
        crate::ir::Instruction::Log(t) => assert_eq!(
            crate::ir::serialize::write_template(t),
            "(t (lit \"step-run\"))"
        ),
        other => panic!("expected bound step log, got {:?}", other),
    }

    let error = compile_error(
        "\
fn step { step(); };

project p {
    step();
};
",
    );
    assert!(
        error.contains("circular function call: step -> step"),
        "{error}"
    );
}
