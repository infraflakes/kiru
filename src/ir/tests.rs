use std::collections::BTreeMap;

use super::*;

/// A small program exercising every node kind and container relationship.
fn sample_program() -> Program {
    let check = Template {
        parts: vec![Segment::Cmd(Template::lit("test -f $HOME"))],
    };
    let mut build = ProgramBuilder::default();

    let exec = build.push(NodeKind::Exec(check.clone()), vec![]);
    let log_switching = build.push(NodeKind::Log(Template::lit("switching")), vec![]);
    let arm_lit = build.push(
        NodeKind::Arm(ArmPattern::Lit("1".to_string())),
        vec![log_switching],
    );
    let log_default = build.push(NodeKind::Log(Template::lit("default")), vec![]);
    let arm_default = build.push(NodeKind::Arm(ArmPattern::Default), vec![log_default]);
    let switch = build.push(NodeKind::Switch(check), vec![arm_lit, arm_default]);

    let cd = build.push(NodeKind::Cd(Template::lit("project")), vec![]);
    let env = build.push(
        NodeKind::Env(vec![EnvPair {
            key: "GO".to_string(),
            value: Template::lit("1"),
        }]),
        vec![cd],
    );
    let project = build.push(
        NodeKind::Project(Template::lit("nix")),
        vec![exec, switch, env],
    );

    let log_concurrent = build.push(NodeKind::Log(Template::lit("concurrent")), vec![]);
    let group = build.push(NodeKind::Async, vec![log_concurrent]);

    build.build(BTreeMap::from([(
        "bootstrap".to_string(),
        vec![project, group],
    )]))
}

#[test]
fn test_program_serialization_round_trip() {
    let program = sample_program();
    let text = program.serialize();
    let parsed = Program::deserialize(&text).expect("should parse");
    assert_eq!(program, parsed, "round trip mismatch:\n{}", text);
}

#[test]
fn test_program_serialization_escapes() {
    let mut build = ProgramBuilder::default();
    let exec = build.push(
        NodeKind::Exec(Template::lit("has \"quotes\" and ) parens")),
        vec![],
    );
    let program = build.build(BTreeMap::from([("p".to_string(), vec![exec])]));

    let text = program.serialize();
    let parsed = Program::deserialize(&text).expect("should parse");
    assert_eq!(parsed, program);
    match &parsed.node(parsed.run_children("p")[0]).kind {
        NodeKind::Exec(template) => {
            assert_eq!(
                template.parts,
                vec![Segment::Lit("has \"quotes\" and ) parens".to_string())]
            )
        }
        other => panic!("expected exec, got {:?}", other),
    }
}

/// The subtree walk reaches every node in pre-order, structural nodes
/// included, and is the single definition the executor marks statuses with.
#[test]
fn test_subtree_walk_covers_every_node_pre_order() {
    let program = sample_program();
    let mut visited = Vec::new();
    program.visit_subtree(program.run_children("bootstrap"), &mut |id| {
        visited.push(id)
    });
    assert_eq!(
        visited.len(),
        program.nodes.len(),
        "every node is visited exactly once"
    );
    visited.sort_unstable();
    assert_eq!(
        visited,
        (0..program.nodes.len()).collect::<Vec<_>>(),
        "the walk covers the whole arena"
    );
}

/// The viewport is sized from the run being shown, not the whole program.
#[test]
fn test_run_row_count_counts_only_that_run() {
    let mut build = ProgramBuilder::default();
    let a = build.push(NodeKind::Log(Template::lit("a")), vec![]);
    let b = build.push(NodeKind::Log(Template::lit("b")), vec![]);
    let c = build.push(NodeKind::Log(Template::lit("c")), vec![]);
    let program = build.build(BTreeMap::from([
        ("one".to_string(), vec![a, b]),
        ("two".to_string(), vec![c]),
    ]));
    assert_eq!(program.run_row_count("one"), 2);
    assert_eq!(program.run_row_count("two"), 1);
    assert_eq!(program.run_row_count("missing"), 0);
}

#[test]
fn test_unknown_root_is_rejected() {
    // Only the program root deserializes; any other root (an old artifact
    // shape among them) must be rejected, not silently tolerated.
    let text = "(legacy\n  (version 1)\n)\n";
    assert!(Program::deserialize(text).is_err());
}

#[test]
fn test_trailing_content_rejected() {
    // Anything after the root s-expression means a truncated or
    // concatenated file; it must never half-load.
    let text = "(program)\n(run rogue (stage (call p f)))\n";
    assert!(Program::deserialize(text).is_err());
}
