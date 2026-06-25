use crate::compiler::{Project, Sanctuary};
use crate::dsl::{CaseArm, CasePattern, Expr, FnStmt, InterpolationPart};
use crate::runner::output::OutputTarget;
use crate::runner::parse::ExecContext;
use std::collections::HashMap;

fn test_context(vars: HashMap<String, String>) -> (Sanctuary, Project, OutputTarget) {
    let project = Project {
        name: "test".to_string(),
        url: "http://example.com".to_string(),
        dir: "test".to_string(),
        sync: "clone".to_string(),
        include_file: None,
        branch: "main".to_string(),
        vars,
        shell_vars: HashMap::new(),
        functions: HashMap::new(),
        runs: HashMap::new(),
    };
    let cfg = Sanctuary {
        sanctuary_path: "/tmp".to_string(),
        projects: HashMap::new(),
        vars: HashMap::new(),
        shell_vars: HashMap::new(),
        functions: HashMap::new(),
        runs: HashMap::new(),
    };
    (cfg, project, OutputTarget::Direct(Box::new(Vec::new())))
}

#[test]
fn test_match_literal_pattern() {
    let vars = HashMap::new();
    let (cfg, project, mut output) = test_context(vars);
    let mut ctx = ExecContext::new(&cfg, Some(&project), &mut output);
    let pattern = CasePattern::Literal {
        parts: vec![InterpolationPart {
            is_var: false,
            value: "Linux".to_string(),
        }],
    };
    assert!(ctx.match_case_pattern(&pattern, "Linux").unwrap());
    assert!(!ctx.match_case_pattern(&pattern, "Darwin").unwrap());
}

#[test]
fn test_match_literal_with_interpolation() {
    let mut vars = HashMap::new();
    vars.insert("arch".to_string(), "amd64".to_string());
    let (cfg, project, mut output) = test_context(vars);
    let mut ctx = ExecContext::new(&cfg, Some(&project), &mut output);
    let pattern = CasePattern::Literal {
        parts: vec![
            InterpolationPart {
                is_var: false,
                value: "linux/".to_string(),
            },
            InterpolationPart {
                is_var: true,
                value: "arch".to_string(),
            },
        ],
    };
    assert!(ctx.match_case_pattern(&pattern, "linux/amd64").unwrap());
    assert!(!ctx.match_case_pattern(&pattern, "linux/arm64").unwrap());
}

#[test]
fn test_match_varref_pattern() {
    let mut vars = HashMap::new();
    vars.insert("expected".to_string(), "hello".to_string());
    let (cfg, project, mut output) = test_context(vars);
    let mut ctx = ExecContext::new(&cfg, Some(&project), &mut output);
    let pattern = CasePattern::VarRef {
        name: "expected".to_string(),
    };
    assert!(ctx.match_case_pattern(&pattern, "hello").unwrap());
    assert!(!ctx.match_case_pattern(&pattern, "world").unwrap());
}

#[test]
fn test_match_default_pattern() {
    let vars = HashMap::new();
    let (cfg, project, mut output) = test_context(vars);
    let mut ctx = ExecContext::new(&cfg, Some(&project), &mut output);
    let pattern = CasePattern::Default;
    assert!(ctx.match_case_pattern(&pattern, "anything").unwrap());
    assert!(ctx.match_case_pattern(&pattern, "").unwrap());
}

#[test]
fn test_match_empty_string() {
    let vars = HashMap::new();
    let (cfg, project, mut output) = test_context(vars);
    let mut ctx = ExecContext::new(&cfg, Some(&project), &mut output);
    let pattern = CasePattern::Literal {
        parts: vec![InterpolationPart {
            is_var: false,
            value: "".to_string(),
        }],
    };
    assert!(ctx.match_case_pattern(&pattern, "").unwrap());
    assert!(!ctx.match_case_pattern(&pattern, "x").unwrap());
}

#[test]
fn test_match_undefined_var_in_literal_pattern() {
    let vars = HashMap::new();
    let (cfg, project, mut output) = test_context(vars);
    let mut ctx = ExecContext::new(&cfg, Some(&project), &mut output);
    let pattern = CasePattern::Literal {
        parts: vec![InterpolationPart {
            is_var: true,
            value: "undefined".to_string(),
        }],
    };
    let result = ctx.match_case_pattern(&pattern, "x");
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("undefined variable")
    );
}

#[test]
fn test_match_undefined_var_in_varref_pattern() {
    let vars = HashMap::new();
    let (cfg, project, mut output) = test_context(vars);
    let mut ctx = ExecContext::new(&cfg, Some(&project), &mut output);
    let pattern = CasePattern::VarRef {
        name: "undefined".to_string(),
    };
    let result = ctx.match_case_pattern(&pattern, "x");
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("undefined variable")
    );
}

#[test]
fn test_case_first_match_wins() {
    let vars = HashMap::new();
    let (cfg, project, mut output) = test_context(vars);
    let mut ctx = ExecContext::new(&cfg, Some(&project), &mut output);
    let body = [FnStmt::Case {
        condition: Expr::BacktickLit {
            offset: 0,
            len: 0,
            parts: vec![InterpolationPart {
                is_var: false,
                value: "a".to_string(),
            }],
        },
        scopes: vec![
            CaseArm {
                pattern: CasePattern::Literal {
                    parts: vec![InterpolationPart {
                        is_var: false,
                        value: "a".to_string(),
                    }],
                },
                body: vec![FnStmt::Log {
                    value: Expr::BacktickLit {
                        offset: 0,
                        len: 0,
                        parts: vec![InterpolationPart {
                            is_var: false,
                            value: "first".to_string(),
                        }],
                    },
                }],
            },
            CaseArm {
                pattern: CasePattern::Default,
                body: vec![FnStmt::Log {
                    value: Expr::BacktickLit {
                        offset: 0,
                        len: 0,
                        parts: vec![InterpolationPart {
                            is_var: false,
                            value: "second".to_string(),
                        }],
                    },
                }],
            },
        ],
    }];
    ctx.exec_fn_body(&body).unwrap();
}

#[test]
fn test_case_no_match_does_nothing() {
    let vars = HashMap::new();
    let (cfg, project, mut output) = test_context(vars);
    let mut ctx = ExecContext::new(&cfg, Some(&project), &mut output);
    let body = [FnStmt::Case {
        condition: Expr::BacktickLit {
            offset: 0,
            len: 0,
            parts: vec![InterpolationPart {
                is_var: false,
                value: "no-match".to_string(),
            }],
        },
        scopes: vec![CaseArm {
            pattern: CasePattern::Literal {
                parts: vec![InterpolationPart {
                    is_var: false,
                    value: "a".to_string(),
                }],
            },
            body: vec![FnStmt::Log {
                value: Expr::BacktickLit {
                    offset: 0,
                    len: 0,
                    parts: vec![InterpolationPart {
                        is_var: false,
                        value: "should-not-run".to_string(),
                    }],
                },
            }],
        }],
    }];
    ctx.exec_fn_body(&body).unwrap();
}
