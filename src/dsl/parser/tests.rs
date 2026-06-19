use super::expr::parse_template_parts;
use super::*;
use crate::dsl::lexer::Lexer;
fn parse_program(input: &str) -> Result<Program, Vec<ParseError>> {
    let lexer = Lexer::new(input.to_string());
    let mut parser = Parser::new(lexer);
    parser.parse()
}

fn count_fn_stmt_types(body: &[FnStmt]) -> Vec<&'static str> {
    body.iter()
        .map(|s| match s {
            FnStmt::Log { .. } => "log",
            FnStmt::Exec { .. } => "exec",
            FnStmt::Cd { .. } => "cd",
            FnStmt::VarDecl { .. } => "var",
            FnStmt::EnvBlock { .. } => "env",
            FnStmt::Case { .. } => "case",
        })
        .collect()
}

fn count_stmt_types(program: &Program) -> Vec<&'static str> {
    program
        .stmts
        .iter()
        .map(|s| match s {
            Stmt::SanctuaryDecl { .. } => "sanctuary",
            Stmt::ImportDecl { .. } => "import",
            Stmt::VarDecl { .. } => "var",
            Stmt::ProjectDecl { .. } => "pr",
            Stmt::FnDecl { .. } => "fn",
            Stmt::RunDecl { .. } => "run",
        })
        .collect()
}

#[test]
fn test_sanctuary_decl() {
    let prog = parse_program("sanctuary = `/tmp/dev`;").unwrap();
    assert_eq!(count_stmt_types(&prog), vec!["sanctuary"]);
    match &prog.stmts[0] {
        Stmt::SanctuaryDecl { value, .. } => match value {
            Expr::BacktickLit { parts, .. } => {
                let concat: String = parts.iter().map(|p| p.value.as_str()).collect();
                assert_eq!(concat, "/tmp/dev");
            }
            _ => panic!("expected BacktickLit"),
        },
        _ => panic!("expected SanctuaryDecl"),
    }
}

#[test]
fn test_sanctuary_with_var_ref() {
    let prog = parse_program("sanctuary = $workdir;").unwrap();
    match &prog.stmts[0] {
        Stmt::SanctuaryDecl { value, .. } => match value {
            Expr::VarRef { name, .. } => assert_eq!(name, "workdir"),
            _ => panic!("expected VarRef"),
        },
        _ => panic!("expected SanctuaryDecl"),
    }
}

#[test]
fn test_import_decl() {
    let prog = parse_program("import `./other.kiru`;").unwrap();
    assert_eq!(count_stmt_types(&prog), vec!["import"]);
    match &prog.stmts[0] {
        Stmt::ImportDecl { path } => match path {
            Expr::BacktickLit { parts, .. } => {
                let concat: String = parts.iter().map(|p| p.value.as_str()).collect();
                assert_eq!(concat, "./other.kiru");
            }
            _ => panic!("expected BacktickLit"),
        },
        _ => panic!("expected ImportDecl"),
    }
}

#[test]
fn test_var_string_decl() {
    let prog = parse_program("var string x = `hello`;").unwrap();
    match &prog.stmts[0] {
        Stmt::VarDecl {
            var_type,
            name,
            value,
            ..
        } => {
            assert_eq!(*var_type, VarType::String);
            assert_eq!(name, "x");
            match value {
                Expr::BacktickLit { parts, .. } => {
                    let concat: String = parts.iter().map(|p| p.value.as_str()).collect();
                    assert_eq!(concat, "hello");
                }
                _ => panic!("expected BacktickLit"),
            }
        }
        _ => panic!("expected VarDecl"),
    }
}

#[test]
fn test_var_shell_decl() {
    let prog = parse_program("var shell x = `echo hello`;").unwrap();
    match &prog.stmts[0] {
        Stmt::VarDecl { var_type, name, .. } => {
            assert_eq!(*var_type, VarType::Shell);
            assert_eq!(name, "x");
        }
        _ => panic!("expected VarDecl"),
    }
}

#[test]
fn test_var_missing_type_annotation() {
    let result = parse_program("var x = `hello`;");
    assert!(result.is_err());
    let errs = result.unwrap_err();
    assert!(
        errs.iter()
            .any(|e| e.to_string().contains("expected 'string' or 'shell'"))
    );
}

#[test]
fn test_var_invalid_type() {
    let result = parse_program("var number x = `5`;");
    assert!(result.is_err());
    let errs = result.unwrap_err();
    assert!(
        errs.iter()
            .any(|e| e.to_string().contains("expected 'string' or 'shell'"))
    );
}

#[test]
fn test_project_decl_with_fields() {
    let input = "\npr todo {\n    url = `git@github.com:user/repo.git`;\n    dir = `todo`;\n    sync = `clone`;\n    include = `./main.kiru`;\n    branch = `main`;\n}";
    let prog = parse_program(input).unwrap();
    assert_eq!(count_stmt_types(&prog), vec!["pr"]);
    match &prog.stmts[0] {
        Stmt::ProjectDecl {
            name, fields, body, ..
        } => {
            assert_eq!(name, "todo");
            assert_eq!(fields.len(), 5);
            assert!(body.is_empty());
            let keys: Vec<&str> = fields.iter().map(|f| f.key.as_str()).collect();
            assert_eq!(keys, vec!["url", "dir", "sync", "include", "branch"]);
        }
        _ => panic!("expected ProjectDecl"),
    }
}

#[test]
fn test_project_decl_with_body_stmts() {
    let input = "\npr todo {\n    url = `git@github.com:user/repo.git`;\n    dir = `todo`;\n    var string app = `todo`;\n    fn build {\n        log `building`;\n    }\n    run release {\n        build;\n    }\n    run ci {\n        build;\n    }\n}";
    let prog = parse_program(input).unwrap();
    match &prog.stmts[0] {
        Stmt::ProjectDecl {
            name, fields, body, ..
        } => {
            assert_eq!(name, "todo");
            assert_eq!(fields.len(), 2);
            assert_eq!(body.len(), 4);
            assert!(matches!(body[0], Stmt::VarDecl { .. }));
            assert!(matches!(body[1], Stmt::FnDecl { .. }));
            assert!(matches!(body[2], Stmt::RunDecl { .. }));
            assert!(matches!(body[3], Stmt::RunDecl { .. }));
        }
        _ => panic!("expected ProjectDecl"),
    }
}

#[test]
fn test_project_duplicate_fields() {
    let input = "\npr x {\n    url = `a`;\n    url = `b`;\n    dir = `d`;\n}";
    let prog = parse_program(input).unwrap();
    match &prog.stmts[0] {
        Stmt::ProjectDecl { fields, .. } => {
            assert_eq!(fields.len(), 3);
        }
        _ => panic!("expected ProjectDecl"),
    }
}

#[test]
fn test_run_only_allows_ident() {
    let result = parse_program("run s { 123; }");
    assert!(result.is_err());
}

#[test]
fn test_run_ref_not_allowed() {
    let result = parse_program("run p { run.x; }");
    assert!(result.is_err());
}

#[test]
fn test_run_name_ref_not_allowed() {
    let result = parse_program("run s { s.x; }");
    assert!(result.is_err());
}

// --- Error recovery tests ---

#[test]
fn test_missing_semicolon() {
    let result = parse_program("sanctuary = `$HOME`");
    assert!(result.is_err());
    let errs = result.unwrap_err();
    assert!(errs.iter().any(|e| e.to_string().contains("expected")));
}

#[test]
fn test_missing_opening_brace_after_fn() {
    let result = parse_program("fn bad");
    assert!(result.is_err());
}

#[test]
fn test_missing_opening_brace_after_run() {
    let result = parse_program("run bad");
    assert!(result.is_err());
}

#[test]
fn test_unexpected_token_at_top_level() {
    let result = parse_program("fooobar = `bar`;");
    assert!(result.is_err());
    let errs = result.unwrap_err();
    assert!(
        errs.iter()
            .any(|e| e.to_string().contains("expected sanctuary"))
    );
}

#[test]
fn test_unclosed_fn_brace() {
    let result = parse_program("fn bad { log `hi`;");
    assert!(result.is_err());
}

#[test]
fn test_unclosed_run_brace() {
    let result = parse_program("run s { check;");
    assert!(result.is_err());
}

#[test]
fn test_var_with_var_ref_value() {
    let input = "var string x = `a`; var string y = `${x}`;";
    let prog = parse_program(input).unwrap();
    assert_eq!(count_stmt_types(&prog), vec!["var", "var"]);
}

#[test]
fn test_multiple_top_level_statements() {
    let input = "sanctuary = `/tmp`;\n\
                  import `./other.kiru`;\n\
                  var string x = `hello`;\n\
                   pr p { url = `u`; dir = `d`; fn f { log `hi`; } run s { f; } }";
    let prog = parse_program(input).unwrap();
    assert_eq!(
        count_stmt_types(&prog),
        vec!["sanctuary", "import", "var", "pr"]
    );
}

#[test]
fn test_error_recovery_skips_bad_stmt() {
    let result = parse_program("sanctuary = `/tmp`;\nfn bad { unknown }");
    match result {
        Ok(prog) => {
            assert_eq!(prog.stmts.len(), 2);
        }
        Err(errs) => {
            assert!(errs.iter().any(|e| e.to_string().contains("expected log")));
        }
    }
}

#[test]
fn test_import_path_types() {
    let inputs = vec![
        "import `./foo.kiru`;",
        "import `../foo.kiru`;",
        "import `../../dir/foo.kiru`;",
    ];
    for input in inputs {
        let result = parse_program(input);
        assert!(result.is_ok(), "expected success for: {}", input);
    }
}

#[test]
fn test_project_with_interleaved_fields_and_body() {
    let input = "\npr todo {\n    url = `u`;\n    var string app = `todo`;\n    dir = `d`;\n    fn build { log `x`; }\n    sync = `clone`;\n}";
    let prog = parse_program(input).unwrap();
    match &prog.stmts[0] {
        Stmt::ProjectDecl { fields, body, .. } => {
            assert_eq!(fields.len(), 3);
            assert_eq!(body.len(), 2);
        }
        _ => panic!("expected ProjectDecl"),
    }
}

// --- Case statement tests ---

#[test]
fn test_case_stmt_in_fn_body() {
    let input = "pr p { fn test { case $os { `Linux` { log `linux`; }; _ { log `other`; }; }; } }";
    let prog = parse_program(input).unwrap();
    match &prog.stmts[0] {
        Stmt::ProjectDecl { body, .. } => match &body[0] {
            Stmt::FnDecl { name, body, .. } => {
                assert_eq!(name, "test");
                assert_eq!(count_fn_stmt_types(body), vec!["case"]);
                match &body[0] {
                    FnStmt::Case {
                        condition, arms, ..
                    } => {
                        assert!(matches!(condition, Expr::VarRef { .. }));
                        assert_eq!(arms.len(), 2);
                        assert!(matches!(arms[0].pattern, CasePattern::Literal { .. }));
                        assert!(matches!(arms[1].pattern, CasePattern::Default));
                    }
                    _ => panic!("expected Case"),
                }
            }
            _ => panic!("expected FnDecl"),
        },
        _ => panic!("expected ProjectDecl"),
    }
}

#[test]
fn test_case_with_var_ref_pattern() {
    let input =
        "pr p { fn test { case $os { $expected { log `match`; }; _ { log `no match`; }; }; } }";
    let prog = parse_program(input).unwrap();
    match &prog.stmts[0] {
        Stmt::ProjectDecl { body, .. } => match &body[0] {
            Stmt::FnDecl { body, .. } => match &body[0] {
                FnStmt::Case { arms, .. } => {
                    assert_eq!(arms.len(), 2);
                    assert!(matches!(arms[0].pattern, CasePattern::VarRef { .. }));
                    assert!(matches!(arms[1].pattern, CasePattern::Default));
                }
                _ => panic!("expected Case"),
            },
            _ => panic!("expected FnDecl"),
        },
        _ => panic!("expected ProjectDecl"),
    }
}

#[test]
fn test_case_with_backtick_condition() {
    let input = "pr p { fn test { case `hello` { `hello` { log `match`; }; _ { log `no`; }; }; } }";
    let prog = parse_program(input).unwrap();
    match &prog.stmts[0] {
        Stmt::ProjectDecl { body, .. } => match &body[0] {
            Stmt::FnDecl { body, .. } => match &body[0] {
                FnStmt::Case { condition, .. } => {
                    assert!(matches!(condition, Expr::BacktickLit { .. }));
                }
                _ => panic!("expected Case"),
            },
            _ => panic!("expected FnDecl"),
        },
        _ => panic!("expected ProjectDecl"),
    }
}

#[test]
fn test_case_with_interpolation_in_pattern() {
    let input =
        "pr p { fn test { case $os { `hello ${world}` { log `match`; }; _ { log `no`; }; }; } }";
    let prog = parse_program(input).unwrap();
    match &prog.stmts[0] {
        Stmt::ProjectDecl { body, .. } => match &body[0] {
            Stmt::FnDecl { body, .. } => match &body[0] {
                FnStmt::Case { arms, .. } => {
                    assert!(matches!(arms[0].pattern, CasePattern::Literal { .. }));
                }
                _ => panic!("expected Case"),
            },
            _ => panic!("expected FnDecl"),
        },
        _ => panic!("expected ProjectDecl"),
    }
}

#[test]
fn test_case_nested_inside_env() {
    let input = "pr p { fn test { env [DEBUG = `1`] { case $os { `Linux` { log `linux`; }; _ { log `other`; }; }; }; } }";
    let prog = parse_program(input).unwrap();
    match &prog.stmts[0] {
        Stmt::ProjectDecl { body, .. } => match &body[0] {
            Stmt::FnDecl { body, .. } => {
                assert_eq!(count_fn_stmt_types(body), vec!["env"]);
                match &body[0] {
                    FnStmt::EnvBlock { body: env_body, .. } => {
                        assert_eq!(count_fn_stmt_types(env_body), vec!["case"]);
                    }
                    _ => panic!("expected EnvBlock"),
                }
            }
            _ => panic!("expected FnDecl"),
        },
        _ => panic!("expected ProjectDecl"),
    }
}

#[test]
fn test_case_missing_opening_brace_error() {
    let result = parse_program("pr p { fn test { case $os _ { log `x`; }; } }");
    let errs = result.unwrap_err();
    assert!(
        errs.iter().any(|e| e.to_string().contains("expected `{`")),
        "got: {:?}",
        errs
    );
}

#[test]
fn test_case_missing_semicolon_after_arm() {
    let result = parse_program("pr p { fn test { case $os { `a` { log `x`; } } } }");
    let errs = result.unwrap_err();
    assert!(
        errs.iter().any(|e| e.to_string().contains("expected `;`")),
        "got: {:?}",
        errs
    );
}

#[test]
fn test_case_pattern_invalid() {
    let result = parse_program("pr p { fn test { case $os { 123 { log `x`; }; } } }");
    let errs = result.unwrap_err();
    assert!(
        errs.iter()
            .any(|e| e.to_string().contains("expected pattern before")),
        "got: {:?}",
        errs
    );
}

#[test]
fn test_case_missing_semicolon_after_block() {
    let result = parse_program("pr p { fn test { case $os { _ { log `x`; }; } } }");
    let errs = result.unwrap_err();
    assert!(
        errs.iter().any(|e| e.to_string().contains("expected `;`")),
        "got: {:?}",
        errs
    );
}

#[test]
fn test_case_nested_inside_case() {
    let input = "pr p { fn test { case $x { `a` { case $y { `1` { log `nested`; }; _ { }; }; }; _ { }; }; } }";
    let prog = parse_program(input).unwrap();
    match &prog.stmts[0] {
        Stmt::ProjectDecl { body, .. } => match &body[0] {
            Stmt::FnDecl { body, .. } => {
                assert_eq!(count_fn_stmt_types(body), vec!["case"]);
                match &body[0] {
                    FnStmt::Case { arms, .. } => {
                        assert_eq!(arms.len(), 2);
                        assert_eq!(count_fn_stmt_types(&arms[0].body), vec!["case"]);
                        assert!(arms[1].body.is_empty());
                    }
                    _ => panic!("expected Case"),
                }
            }
            _ => panic!("expected FnDecl"),
        },
        _ => panic!("expected ProjectDecl"),
    }
}

#[test]
fn test_case_with_cd_in_arm() {
    let input = "pr p { fn test { case $x { `a` { cd `dir`; }; _ { cd $x; }; }; } }";
    let prog = parse_program(input).unwrap();
    match &prog.stmts[0] {
        Stmt::ProjectDecl { body, .. } => match &body[0] {
            Stmt::FnDecl { body, .. } => match &body[0] {
                FnStmt::Case { arms, .. } => {
                    assert_eq!(count_fn_stmt_types(&arms[0].body), vec!["cd"]);
                    assert_eq!(count_fn_stmt_types(&arms[1].body), vec!["cd"]);
                }
                _ => panic!("expected Case"),
            },
            _ => panic!("expected FnDecl"),
        },
        _ => panic!("expected ProjectDecl"),
    }
}

#[test]
fn test_case_with_var_decl_in_arm() {
    let input = "pr p { fn test { case $x { `a` { var string msg = `hello`; }; _ { }; }; } }";
    let prog = parse_program(input).unwrap();
    match &prog.stmts[0] {
        Stmt::ProjectDecl { body, .. } => match &body[0] {
            Stmt::FnDecl { body, .. } => match &body[0] {
                FnStmt::Case { arms, .. } => {
                    assert_eq!(count_fn_stmt_types(&arms[0].body), vec!["var"]);
                }
                _ => panic!("expected Case"),
            },
            _ => panic!("expected FnDecl"),
        },
        _ => panic!("expected ProjectDecl"),
    }
}

#[test]
fn test_case_with_env_in_arm() {
    let input =
        "pr p { fn test { case $x { `a` { env [DEBUG = `1`] { log `ok`; }; }; _ { }; }; } }";
    let prog = parse_program(input).unwrap();
    match &prog.stmts[0] {
        Stmt::ProjectDecl { body, .. } => match &body[0] {
            Stmt::FnDecl { body, .. } => match &body[0] {
                FnStmt::Case { arms, .. } => {
                    assert_eq!(count_fn_stmt_types(&arms[0].body), vec!["env"]);
                }
                _ => panic!("expected Case"),
            },
            _ => panic!("expected FnDecl"),
        },
        _ => panic!("expected ProjectDecl"),
    }
}

#[test]
fn test_case_arm_empty_body() {
    let input = "pr p { fn test { case $x { _ { }; }; } }";
    let prog = parse_program(input).unwrap();
    match &prog.stmts[0] {
        Stmt::ProjectDecl { body, .. } => match &body[0] {
            Stmt::FnDecl { body, .. } => match &body[0] {
                FnStmt::Case { arms, .. } => {
                    assert!(arms[0].body.is_empty());
                }
                _ => panic!("expected Case"),
            },
            _ => panic!("expected FnDecl"),
        },
        _ => panic!("expected ProjectDecl"),
    }
}

#[test]
fn test_case_duplicate_default() {
    let input = "pr p { fn test { case $x { _ { log `a`; }; _ { log `b`; }; }; } }";
    let prog = parse_program(input).unwrap();
    match &prog.stmts[0] {
        Stmt::ProjectDecl { body, .. } => match &body[0] {
            Stmt::FnDecl { body, .. } => match &body[0] {
                FnStmt::Case { arms, .. } => {
                    assert_eq!(arms.len(), 2);
                    assert!(matches!(arms[0].pattern, CasePattern::Default));
                    assert!(matches!(arms[1].pattern, CasePattern::Default));
                }
                _ => panic!("expected Case"),
            },
            _ => panic!("expected FnDecl"),
        },
        _ => panic!("expected ProjectDecl"),
    }
}

#[test]
fn test_underscore_outside_case_pattern() {
    let result = parse_program("pr p { fn test { log `_`; _; } }");
    let errs = result.unwrap_err();
    assert!(
        errs.iter().any(|e| e
            .to_string()
            .contains("`_` is only valid as a case pattern")),
        "got: {:?}",
        errs
    );
}

#[test]
fn test_case_with_exec_and_log() {
    let input = "pr p {
    fn deploy {
        var shell docker_bin = `command -v docker 2>/dev/null || command -v podman 2>/dev/null`;
        case `${docker_bin}` {
             `` { log `no container runtime found`; };
            _ { exec `${docker_bin} build .`; };
        };
    }
}";
    let prog = parse_program(input).unwrap();
    match &prog.stmts[0] {
        Stmt::ProjectDecl { body, .. } => match &body[0] {
            Stmt::FnDecl { name, body, .. } => {
                assert_eq!(name, "deploy");
                assert_eq!(count_fn_stmt_types(body), vec!["var", "case"]);
                match &body[1] {
                    FnStmt::Case { condition, arms } => {
                        assert!(matches!(condition, Expr::BacktickLit { .. }));
                        assert_eq!(arms.len(), 2);
                        assert!(matches!(arms[0].pattern, CasePattern::Literal { .. }));
                        assert!(matches!(arms[1].pattern, CasePattern::Default));
                        assert_eq!(count_fn_stmt_types(&arms[0].body), vec!["log"]);
                        assert_eq!(count_fn_stmt_types(&arms[1].body), vec!["exec"]);
                    }
                    _ => panic!("expected Case"),
                }
            }
            _ => panic!("expected FnDecl"),
        },
        _ => panic!("expected ProjectDecl"),
    }
}

// --- Template part tests ---

#[test]
fn test_basic_template_part() {
    let parts = parse_template_parts("hello", 0).unwrap();
    assert_eq!(parts.len(), 1);
    assert!(!parts[0].is_var);
    assert_eq!(parts[0].value, "hello");
}

#[test]
fn test_template_with_var() {
    let parts = parse_template_parts("hello ${name} world", 0).unwrap();
    assert_eq!(parts.len(), 3);
    assert!(!parts[0].is_var);
    assert_eq!(parts[0].value, "hello ");
    assert!(parts[1].is_var);
    assert_eq!(parts[1].value, "name");
    assert!(!parts[2].is_var);
    assert_eq!(parts[2].value, " world");
}

#[test]
fn test_template_empty_var_name() {
    let result = parse_template_parts("hello ${}", 0);
    assert!(result.is_err());
    assert!(
        result
            .unwrap_err()
            .to_string()
            .contains("empty variable name")
    );
}
