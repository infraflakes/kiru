use crate::compiler::types::ResolvedCaseArm;
use crate::compiler::{Project, ResolvedCasePattern, ResolvedFnStmt, Sanctuary, SyncMode};
use crate::runner::output::OutputTarget;
use crate::runner::parse::{ExecContext, match_case_pattern};
use std::collections::HashMap;

fn test_context() -> (Sanctuary, Project, OutputTarget) {
    let project = Project {
        url: "http://example.com".to_string(),
        dir: "test".to_string(),
        sync: SyncMode::Clone,
        branch: Some("main".to_string()),
        functions: HashMap::new(),
        runs: HashMap::new(),
    };
    let cfg = Sanctuary {
        sanctuary_path: "/tmp".to_string(),
        projects: HashMap::new(),
        functions: HashMap::new(),
        runs: HashMap::new(),
    };
    (cfg, project, OutputTarget::Direct(Box::new(Vec::new())))
}

#[test]
fn test_match_literal_pattern() {
    let pattern = ResolvedCasePattern::Literal("Linux".to_string());
    assert!(match_case_pattern(&pattern, "Linux"));
    assert!(!match_case_pattern(&pattern, "Darwin"));
}

#[test]
fn test_match_default_pattern() {
    let pattern = ResolvedCasePattern::Default;
    assert!(match_case_pattern(&pattern, "anything"));
    assert!(match_case_pattern(&pattern, ""));
}

#[test]
fn test_match_empty_string() {
    let pattern = ResolvedCasePattern::Literal(String::new());
    assert!(match_case_pattern(&pattern, ""));
    assert!(!match_case_pattern(&pattern, "x"));
}

#[test]
fn test_case_first_match_wins() {
    let (cfg, project, mut output) = test_context();
    let mut ctx = ExecContext::new(&cfg, Some(&project), &mut output);
    let body = [ResolvedFnStmt::Case {
        condition: "a".to_string(),
        scopes: vec![
            ResolvedCaseArm {
                pattern: ResolvedCasePattern::Literal("a".to_string()),
                body: vec![ResolvedFnStmt::Log {
                    value: "first".to_string(),
                }],
            },
            ResolvedCaseArm {
                pattern: ResolvedCasePattern::Default,
                body: vec![ResolvedFnStmt::Log {
                    value: "second".to_string(),
                }],
            },
        ],
    }];
    ctx.exec_resolved_fn_body(&body).unwrap();
}

#[test]
fn test_case_no_match_does_nothing() {
    let (cfg, project, mut output) = test_context();
    let mut ctx = ExecContext::new(&cfg, Some(&project), &mut output);
    let body = [ResolvedFnStmt::Case {
        condition: "no-match".to_string(),
        scopes: vec![ResolvedCaseArm {
            pattern: ResolvedCasePattern::Literal("a".to_string()),
            body: vec![ResolvedFnStmt::Log {
                value: "should-not-run".to_string(),
            }],
        }],
    }];
    ctx.exec_resolved_fn_body(&body).unwrap();
}
