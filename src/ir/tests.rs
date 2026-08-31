use std::collections::BTreeMap;

use super::*;

fn sample_ir() -> Ir {
    let check_cmd = Template {
        segments: vec![Segment::Command(Template::lit("test -f $HOME"))],
    };

    let mut project = Project::default();
    project.functions.insert(
        "ssh".to_string(),
        vec![
            Instruction::Exec {
                value: check_cmd.clone(),
            },
            Instruction::Switch {
                subject: check_cmd,
                arms: vec![
                    Arm {
                        pattern: ArmPattern::Lit("1".to_string()),
                        body: vec![Instruction::Log(Template::lit("switching"))],
                    },
                    Arm {
                        pattern: ArmPattern::Default,
                        body: vec![Instruction::Log(Template::lit("default"))],
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
        ],
    );

    let mut repositories = BTreeMap::new();
    repositories.insert(
        "nix".to_string(),
        Sync {
            url: Template::lit("https://example.com/nix"),
            dir: Template::lit("/home/me/nix"),
            branch: Template::lit("main"),
            strategy: Template::lit("clone"),
        },
    );

    let mut execution_chains = BTreeMap::new();
    execution_chains.insert(
        "bootstrap".to_string(),
        vec![vec![Call {
            project: "nix".to_string(),
            function: "ssh".to_string(),
        }]],
    );

    Ir {
        shell: "sh".to_string(),
        timeout: 30,
        repositories,
        projects: {
            let mut m = BTreeMap::new();
            m.insert("nix".to_string(), project);
            m
        },
        execution_chains,
    }
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
    let mut ir = Ir {
        shell: "sh".to_string(),
        ..Default::default()
    };
    let mut project = Project::default();
    project.functions.insert(
        "weird".to_string(),
        vec![Instruction::Exec {
            value: Template::lit("has \"quotes\" and ) parens"),
        }],
    );
    ir.projects.insert("p".to_string(), project);
    let text = ir.serialize();
    let parsed = Ir::deserialize(&text).expect("should parse");
    assert_eq!(
        parsed.projects["p"].functions["weird"][0],
        Instruction::Exec {
            value: Template::lit("has \"quotes\" and ) parens"),
        }
    );
}
