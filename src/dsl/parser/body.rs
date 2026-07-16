use super::expr::parse_interpolation_parts;
use super::*;

impl Parser {
    pub(crate) fn parse_log_stmt(&mut self) -> Result<FnStmt, ParseError> {
        self.advance();

        let value = self.parse_expr()?;
        self.expect_with_context(TokenType::Semicolon, "after `log`")?;

        Ok(FnStmt::Log { value })
    }

    pub(crate) fn parse_exec_stmt(&mut self) -> Result<FnStmt, ParseError> {
        self.advance();

        let value = self.parse_expr()?;
        self.expect_with_context(TokenType::Semicolon, "after `exec`")?;

        Ok(FnStmt::Exec { value })
    }

    pub(crate) fn parse_cd_stmt(&mut self) -> Result<FnStmt, ParseError> {
        self.advance();

        let arg = self.parse_expr()?;
        self.expect_with_context(TokenType::Semicolon, "after `cd`")?;

        Ok(FnStmt::Cd { value: arg })
    }

    pub(crate) fn parse_fn_var_decl(&mut self) -> Result<FnStmt, ParseError> {
        let (var_type, name, value) = self.parse_var_decl_common()?;
        Ok(FnStmt::VarDecl {
            var_type,
            name,
            value,
        })
    }

    pub(crate) fn parse_env_block(&mut self) -> Result<FnStmt, ParseError> {
        self.advance();

        self.expect_with_context(TokenType::LBracket, "after `env`")?;

        let mut pairs = Vec::new();
        while self.current_token().ty != TokenType::RBracket {
            let key = match &self.current_token().ty {
                TokenType::Ident(key_str) => key_str.clone(),
                ty if is_keyword_token(ty) => {
                    return Err(ParseError::new(
                        self.eof_aware_span(),
                        format!(
                            "expected identifier in env pair, found {} (reserved keyword)",
                            format_token(self.current_token())
                        ),
                    ));
                }
                _ => {
                    return Err(ParseError::new(
                        self.eof_aware_span(),
                        "expected identifier in env pair".to_string(),
                    ));
                }
            };
            self.advance();

            self.expect_with_context(TokenType::Assign, "in env pair")?;

            let value = self.parse_expr()?;
            pairs.push(EnvPair { key, value });

            if self.current_token().ty == TokenType::RBracket {
                break;
            }
        }
        self.expect_with_context(TokenType::RBracket, "to close env pairs")?;

        self.expect_with_context(TokenType::LBrace, "to open env block body")?;

        let mut body = Vec::new();
        while self.current_token().ty != TokenType::RBrace {
            body.push(self.parse_fn_stmt()?);
        }
        self.expect_with_context(TokenType::RBrace, "to close env block body")?;

        self.expect_with_context(TokenType::Semicolon, "after env block")?;

        Ok(FnStmt::EnvBlock { pairs, body })
    }

    pub(crate) fn parse_case_stmt(&mut self) -> Result<FnStmt, ParseError> {
        self.advance();

        let condition = self.parse_expr()?;

        self.expect_with_context(TokenType::LBrace, "to open case arms")?;

        let mut scopes = Vec::new();
        while self.current_token().ty != TokenType::RBrace {
            let pattern = self.parse_case_pattern()?;

            self.expect_with_context(TokenType::LBrace, "after case pattern")?;
            let mut body = Vec::new();
            while self.current_token().ty != TokenType::RBrace {
                body.push(self.parse_fn_stmt()?);
            }
            self.expect_with_context(TokenType::RBrace, "to close case arm body")?;

            self.expect_with_context(TokenType::Semicolon, "after case arm")?;

            scopes.push(CaseArm { pattern, body });
        }
        self.expect_with_context(TokenType::RBrace, "to close case block")?;

        self.expect_with_context(TokenType::Semicolon, "after case block")?;

        Ok(FnStmt::Case { condition, scopes })
    }

    fn parse_case_pattern(&mut self) -> Result<CasePattern, ParseError> {
        let tok = self.current_token().clone();
        match &tok.ty {
            TokenType::Ident(ident) if ident == "_" => {
                self.advance();
                Ok(CasePattern::Default)
            }
            TokenType::Dollar => {
                let start_offset = self.current_token().offset;
                let (name, end_offset) = self.parse_dollar_var_name(
                    start_offset,
                    "expected identifier after `$` in case pattern",
                    "expected identifier after `$` in case pattern",
                )?;
                Ok(CasePattern::VarRef {
                    name,
                    offset: start_offset,
                    len: end_offset - start_offset,
                    source_name: self.source_name.clone(),
                })
            }
            TokenType::Backtick(content) => {
                let offset = tok.offset;
                let len = tok.len;
                self.advance();
                let parts = parse_interpolation_parts(content, offset)?;
                Ok(CasePattern::Literal {
                    parts,
                    offset,
                    len,
                    source_name: self.source_name.clone(),
                })
            }
            _ => {
                let token_str = format_token(&tok);
                Err(ParseError::new(
                    self.eof_aware_span(),
                    format!(
                        "expected pattern before {}; are you missing a case arm pattern?",
                        token_str
                    ),
                ))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::dsl::parser::test_support::*;
    use crate::dsl::{FnStmt, Stmt, TopLevel};

    #[test]
    fn test_fn_body_with_log_exec() {
        let input = "fn build {\n\
                      log `compiling`;\n\
                      exec `cargo build`;\n\
                      }";
        let prog = parse_program(input).unwrap();
        match &prog.items[0] {
            TopLevel::Stmt(Stmt::Fn { name, body, .. }) => {
                assert_eq!(name, "build");
                assert_eq!(count_fn_stmt_types(body), vec!["log", "exec"]);
            }
            _ => panic!("expected FnDecl"),
        }
    }

    #[test]
    fn test_fn_body_orders() {
        let input = "fn deploy {\n\
                      env [] { exec `deploy`; };\n\
                      cd `./dist`;\n\
                      exec `npm publish`;\n\
                      log `done`;\n\
                      }";
        let prog = parse_program(input).unwrap();
        match &prog.items[0] {
            TopLevel::Stmt(Stmt::Fn { name, body, .. }) => {
                assert_eq!(name, "deploy");
                assert_eq!(count_fn_stmt_types(body), vec!["env", "cd", "exec", "log"]);
            }
            _ => panic!("expected FnDecl"),
        }
    }

    #[test]
    fn test_env_block_contents() {
        let input = "fn test {\n\
                      env [] {\n\
                      exec `run tests`;\n\
                      log `testing`;\n\
                      };\n\
                      }";
        let prog = parse_program(input).unwrap();
        match &prog.items[0] {
            TopLevel::Stmt(Stmt::Fn { body, .. }) => {
                assert_eq!(body.len(), 1);
                match &body[0] {
                    FnStmt::EnvBlock { body: env_body, .. } => {
                        assert_eq!(env_body.len(), 2);
                        assert!(matches!(env_body[0], FnStmt::Exec { .. }));
                        assert!(matches!(env_body[1], FnStmt::Log { .. }));
                    }
                    _ => panic!("expected EnvBlock"),
                }
            }
            _ => panic!("expected FnDecl"),
        }
    }

    #[test]
    fn test_case_single_branch() {
        let input = "fn test {\n\
                      case $os {\n\
                      `Linux` { exec `linux-deploy`; };\n\
                      };\n\
                      }";
        let prog = parse_program(input).unwrap();
        match &prog.items[0] {
            TopLevel::Stmt(Stmt::Fn { body, .. }) => {
                assert_eq!(body.len(), 1);
                match &body[0] {
                    FnStmt::Case { scopes, .. } => {
                        assert_eq!(scopes.len(), 1);
                    }
                    _ => panic!("expected Case"),
                }
            }
            _ => panic!("expected FnDecl"),
        }
    }

    #[test]
    fn test_case_multiple_branches_with_default() {
        let input = "fn deploy {\n\
                      case $target {\n\
                      `production` { exec `deploy-prod`; };\n\
                      `staging` { exec `deploy-staging`; };\n\
                      _ { log `unknown target`; };\n\
                      };\n\
                      }";
        let prog = parse_program(input).unwrap();
        match &prog.items[0] {
            TopLevel::Stmt(Stmt::Fn { body, .. }) => match &body[0] {
                FnStmt::Case { scopes, .. } => {
                    assert_eq!(scopes.len(), 3);
                    assert!(matches!(
                        &scopes[0].pattern,
                        crate::dsl::syntax::CasePattern::Literal { .. }
                    ));
                    assert!(matches!(
                        &scopes[2].pattern,
                        crate::dsl::syntax::CasePattern::Default
                    ));
                }
                _ => panic!("expected Case"),
            },
            _ => panic!("expected FnDecl"),
        }
    }

    #[test]
    fn test_case_nested() {
        let input = "fn test {\n\
                      case $os {\n\
                      `Linux` {\n\
                      case $arch {\n\
                      `x86_64` { exec `linux-amd64`; };\n\
                      `aarch64` { exec `linux-arm64`; };\n\
                      };\n\
                      };\n\
                      };\n\
                      }";
        let prog = parse_program(input).unwrap();
        match &prog.items[0] {
            TopLevel::Stmt(Stmt::Fn { body, .. }) => match &body[0] {
                FnStmt::Case { scopes, .. } => {
                    assert_eq!(scopes.len(), 1);
                    match &scopes[0].body[0] {
                        FnStmt::Case { scopes: inner, .. } => {
                            assert_eq!(inner.len(), 2);
                        }
                        _ => panic!("expected nested Case"),
                    }
                }
                _ => panic!("expected Case"),
            },
            _ => panic!("expected FnDecl"),
        }
    }

    #[test]
    fn test_cd_statement() {
        let input = "fn build {\n\
                      cd `./src`;\n\
                      }";
        let prog = parse_program(input).unwrap();
        match &prog.items[0] {
            TopLevel::Stmt(Stmt::Fn { body, .. }) => {
                assert_eq!(body.len(), 1);
                assert!(matches!(body[0], FnStmt::Cd { .. }));
            }
            _ => panic!("expected FnDecl"),
        }
    }

    #[test]
    fn test_empty_fn_body() {
        let input = "fn empty {\n\
                      }";
        let prog = parse_program(input).unwrap();
        match &prog.items[0] {
            TopLevel::Stmt(Stmt::Fn { name, body, .. }) => {
                assert_eq!(name, "empty");
                assert!(body.is_empty());
            }
            _ => panic!("expected FnDecl"),
        }
    }

    #[test]
    fn test_trailing_semicolon_in_fn_body() {
        let input = "fn test {\n\
                      log `hi`;\n\
                      log `bye`;\n\
                      }";
        let prog = parse_program(input).unwrap();
        match &prog.items[0] {
            TopLevel::Stmt(Stmt::Fn { body, .. }) => {
                assert_eq!(body.len(), 2);
            }
            _ => panic!("expected FnDecl"),
        }
    }

    #[test]
    fn test_env_block_must_end_with_semicolon() {
        let input = "fn test {\n\
                      env [] { exec `x`; }\n\
                      }";
        let result = parse_program(input);
        match result {
            Ok(_) => panic!("expected error for missing semicolon after env block"),
            Err(errs) => {
                assert!(
                    errs.iter().any(|e| e.to_string().contains("expected")),
                    "got: {:?}",
                    errs
                );
            }
        }
    }

    #[test]
    fn test_log_must_end_with_semicolon() {
        let result = parse_program("fn x { log `hi` }");
        assert!(result.is_err());
    }

    #[test]
    fn test_exec_must_end_with_semicolon() {
        let result = parse_program("fn x { exec `hi` }");
        assert!(result.is_err());
    }
}
