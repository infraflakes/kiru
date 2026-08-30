#[test]
fn test_compile_basic_project() {
    let ir = crate::lower::test_support::compile_str(
        "\
shell = (sh);
var home_dir = $(echo /home/user);
sync nix {
    url = (git@github.com:nix);
    dir = (@(home_dir)/nix);
    branch = (main);
    sync = (clone);
};
pr nix {
    var channel = (unstable);
    fn eval { log (evaluating @(channel)); };
};
run bootstrap { nix::eval; };
",
    );
    use crate::ir::{Instruction, write_template};
    assert_eq!(ir.shell, "sh");
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
    let sync = ir.repositories.get("nix").expect("nix sync");
    // `@(home_dir)` is inlined into the dir template as the preserved command;
    // nothing is executed or frozen at compile time.
    assert_eq!(
        write_template(&sync.url),
        "(t (lit \"git@github.com:nix\"))"
    );
    assert_eq!(
        write_template(&sync.dir),
        "(t (cmd (t (lit \"echo /home/user\"))) (lit \"/nix\"))"
    );
    assert_eq!(write_template(&sync.branch), "(t (lit \"main\"))");
    assert_eq!(write_template(&sync.strategy), "(t (lit \"clone\"))");
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
        "pr nix { fn eval { log (x); }; } run bad { nix::missing; };",
    )
    .unwrap();
    let result = crate::lower::lower_and_resolve(&file, false);
    let _ = std::fs::remove_file(&file);
    assert!(result.is_err());
}

#[test]
fn test_compile_switch_lowering() {
    let ir = crate::lower::test_support::compile_str(
        "\
pr p {
    var os = (linux);
    fn pick {
        switch @(os) {
            case (linux) { log (linux-path); };
            case _ { log (other); };
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
