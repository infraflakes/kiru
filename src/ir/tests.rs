use std::collections::BTreeMap;

use super::*;

fn sample_ir() -> Ir {
    let check_cmd = Template {
        parts: vec![Segment::Cmd(Template::lit("test -f $HOME"))],
    };

    let ssh_body = vec![
        Instruction::Exec {
            command: check_cmd.clone(),
        },
        Instruction::Switch {
            subject: check_cmd,
            arms: vec![
                Arm {
                    row: 1,
                    pattern: ArmPattern::Lit("1".to_string()),
                    body: vec![Instruction::Step {
                        row: 2,
                        label: "log switching".to_string(),
                        body: vec![Instruction::Log(Template::lit("switching"))],
                    }],
                },
                Arm {
                    row: 3,
                    pattern: ArmPattern::Default,
                    body: vec![Instruction::Step {
                        row: 4,
                        label: "log default".to_string(),
                        body: vec![Instruction::Log(Template::lit("default"))],
                    }],
                },
            ],
        },
        Instruction::Env {
            pairs: vec![EnvPair {
                key: "GO".to_string(),
                value: Template::lit("1"),
            }],
            body: vec![Instruction::Cd(Template::lit("project"))],
        },
    ];

    let mut runs = BTreeMap::new();
    runs.insert(
        "bootstrap".to_string(),
        vec![
            Instruction::Step {
                row: 0,
                label: "nix::ssh".to_string(),
                body: vec![Instruction::Context {
                    project: Template::lit("nix"),
                    body: ssh_body,
                }],
            },
            Instruction::Async {
                body: vec![Instruction::Step {
                    row: 1,
                    label: "async".to_string(),
                    body: vec![Instruction::Log(Template::lit("concurrent"))],
                }],
            },
        ],
    );

    Ir { runs }
}

#[test]
fn test_kirufile_round_trip() {
    let ir = sample_ir();
    let text = ir.serialize();
    let parsed = Ir::deserialize(&text).expect("should parse");
    assert_eq!(ir, parsed, "round trip mismatch:\n{}", text);
}

#[test]
fn test_kirufile_escapes() {
    let mut ir = Ir::default();
    ir.runs.insert(
        "p".to_string(),
        vec![Instruction::Step {
            row: 0,
            label: "weird \"label\"".to_string(),
            body: vec![Instruction::Exec {
                command: Template::lit("has \"quotes\" and ) parens"),
            }],
        }],
    );
    let text = ir.serialize();
    let parsed = Ir::deserialize(&text).expect("should parse");
    assert_eq!(
        parsed.runs["p"][0],
        Instruction::Step {
            row: 0,
            label: "weird \"label\"".to_string(),
            body: vec![Instruction::Exec {
                command: Template::lit("has \"quotes\" and ) parens"),
            }],
        }
    );
}

/// Tree prefixes: the outermost level lists plain (no branch), nested
/// lines draw connectors, and ancestor bars continue through non-last
/// blocks.
#[test]
fn test_run_plan_tree_prefixes() {
    fn step(row: usize, label: &str) -> Instruction {
        Instruction::Step {
            row,
            label: label.to_string(),
            body: Vec::new(),
        }
    }
    let async_step = |row: usize, child_row: usize, child: &str| Instruction::Step {
        row,
        label: "async".to_string(),
        body: vec![Instruction::Async {
            body: vec![step(child_row, child)],
        }],
    };

    let mut runs = BTreeMap::new();
    runs.insert(
        "d".to_string(),
        vec![
            async_step(0, 1, "exec a"),
            step(2, "log main"),
            async_step(3, 4, "exec b"),
        ],
    );
    let ir = Ir { runs };
    let plan = ir.run_plan("d");
    let rendered: Vec<(&str, &str)> = plan
        .iter()
        .map(|line| (line.prefix.as_str(), line.label.as_str()))
        .collect();
    assert_eq!(
        rendered,
        vec![
            ("", "async"),
            ("│  └─ ", "exec a"),
            ("", "log main"),
            ("", "async"),
            ("   └─ ", "exec b"),
        ]
    );
}

#[test]
fn test_kirufile_version_entry_rejected() {
    // The version marker is gone from the format; kirufiles carrying it
    // (from older builds) must be rejected, not silently tolerated.
    let text = "(kirufile\n  (version 1)\n)\n";
    assert!(Ir::deserialize(text).is_err());
}

#[test]
fn test_kirufile_trailing_content_rejected() {
    // Anything after the root s-expression means a truncated or
    // concatenated file; it must never half-load.
    let text = "(kirufile)\n(run rogue (stage (call p f)))\n";
    assert!(Ir::deserialize(text).is_err());
}
