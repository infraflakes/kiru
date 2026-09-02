//! Function body parser: `log`, `cd`, `var`, `env`, `switch`, and
//! bare `$(cmd)` statements inside `fn { ... }` blocks.

use super::*;
use crate::syntax::fnstmt::{Arm, FnStmt};
use crate::syntax::source::{ArmPattern, EnvPair};

impl Parser {
    /// Parses a keyword-expr-semicolon statement (`log`, `cd`): skips the
    /// keyword, parses the value expression, and expects the terminating `;`.
    fn parse_expr_stmt(&mut self, context: &'static str) -> Result<Template, ParseError> {
        self.advance();
        let value = self.parse_expr()?;
        self.expect_with_context(TokenType::Semicolon, context)?;
        Ok(value)
    }

    pub(crate) fn parse_log_stmt(&mut self) -> Result<FnStmt, ParseError> {
        Ok(FnStmt::Log(self.parse_expr_stmt("after `log`")?))
    }

    pub(crate) fn parse_cd_stmt(&mut self) -> Result<FnStmt, ParseError> {
        Ok(FnStmt::Cd(self.parse_expr_stmt("after `cd`")?))
    }

    /// Parses a bare `$(cmd);` statement. The current token is a template
    /// token; it is consumed and must contain at least one `$(...)` command.
    /// Bare `()` or `@()` values are rejected (use `log`, `cd`, `var`, `env`,
    /// or `switch` instead).
    pub(crate) fn parse_run_shell_cmd_stmt(&mut self) -> Result<FnStmt, ParseError> {
        let value = self.parse_expr()?;
        let has_cmd = value
            .parts
            .iter()
            .any(|p| matches!(p, crate::syntax::source::Part::Cmd(_)));
        if !has_cmd {
            return Err(ParseError::new(
                crate::diagnostics::Span::new(value.offset, value.len.max(1)),
                "bare template is not a statement — wrap the command in $(...) or prefix with log, cd, var, env or switch"
                    .to_string(),
            ));
        }
        self.expect_with_context(TokenType::Semicolon, "after command statement")?;
        Ok(FnStmt::Bind {
            target: None,
            value,
        })
    }

    pub(crate) fn parse_fn_var_decl(&mut self) -> Result<FnStmt, ParseError> {
        let (name, value) = self.parse_var_decl_common()?;
        Ok(FnStmt::Bind {
            target: Some(name),
            value,
        })
    }

    pub(crate) fn parse_env_block(&mut self) -> Result<FnStmt, ParseError> {
        self.advance(); // skip 'env'

        // First brace group: `key = (template);` env pairs.
        self.expect_with_context(TokenType::LBrace, "after `env`")?;
        let mut pairs = Vec::new();
        while self.current_token().token_type != TokenType::RBrace {
            let key = self.parse_ident_name("identifier in env pair")?;
            self.expect_with_context(TokenType::Assign, "in env pair")?;
            let value = self.parse_expr()?;
            self.expect_with_context(TokenType::Semicolon, "after env pair")?;
            pairs.push(EnvPair { key, value });
        }
        self.expect_with_context(TokenType::RBrace, "to close env pairs")?;

        // Second brace group: the body executed with those env vars exported.
        let body = self.parse_braced_block(
            "to open env block body",
            "to close env block body",
            Self::parse_fn_stmt,
        )?;

        self.expect_with_context(TokenType::Semicolon, "after env block")?;

        Ok(FnStmt::EnvBlock { pairs, body })
    }

    /// Parses a `switch`/`case` block: `switch cond { case (pat) { ... } case _ { ... } };`.
    pub(crate) fn parse_switch_stmt(&mut self) -> Result<FnStmt, ParseError> {
        self.advance(); // skip `switch`

        let subject = self.parse_expr()?;

        self.expect_with_context(TokenType::LBrace, "to open switch arms")?;

        let mut arms = Vec::new();
        while self.current_token().token_type != TokenType::RBrace {
            self.expect_with_context(TokenType::Case, "to open switch arm")?;
            let pattern = self.parse_case_pattern()?;

            let body = self.parse_braced_block(
                "after switch pattern",
                "to close switch arm body",
                Self::parse_fn_stmt,
            )?;

            self.expect_with_context(TokenType::Semicolon, "after switch arm")?;

            arms.push(Arm { pattern, body });
        }
        self.expect_with_context(TokenType::RBrace, "to close switch block")?;

        self.expect_with_context(TokenType::Semicolon, "after switch block")?;

        Ok(FnStmt::Switch { subject, arms })
    }

    fn parse_case_pattern(&mut self) -> Result<ArmPattern, ParseError> {
        if matches!(self.current_token().token_type, TokenType::Ident(ref ident) if ident == "_") {
            self.advance();
            return Ok(ArmPattern::Default);
        }
        let template = self.parse_expr()?;
        // Patterns are matched against the resolved subject string, so only
        // literal text is meaningful: `@(var)` would compare against the
        // variable's name, not its value, and `$(cmd)` has no static text.
        let pattern_is_literal = template
            .parts
            .iter()
            .all(|p| matches!(p, crate::syntax::source::Part::Lit(_)));
        if !pattern_is_literal {
            return Err(ParseError::new(
                crate::diagnostics::Span::new(template.offset, template.len.max(1)),
                "case pattern must be literal text or `_`".to_string(),
            ));
        }
        Ok(ArmPattern::Lit(template.literal_text()))
    }
}

#[cfg(test)]
mod tests {
    use crate::syntax::fnstmt::FnStmt;
    use crate::syntax::parser::test_support::*;
    use crate::syntax::source::ArmPattern;
    use crate::syntax::Stmt;

    /// Parse a wrapped function body: tests describe `fn` bodies, which the
    /// grammar only allows inside a `pr` block.
    fn parse_fn_body(input: &str) -> Vec<FnStmt> {
        let prog = parse_program(input).unwrap();
        match &prog.top_level_items[0] {
            crate::syntax::TopLevel::Stmt(Stmt::Project { body, .. }) => match &body[0] {
                Stmt::Fn { body, .. } => body.clone(),
                other => panic!("expected fn in project body, got {:?}", other),
            },
            other => panic!("expected project, got {:?}", other),
        }
    }

    #[test]
    fn test_fn_body_with_log_exec() {
        let body = parse_fn_body(
            "pr t {\
              fn build {\
                log (compiling);\
                $(cargo build);\
              };\
            };",
        );
        assert_eq!(count_fn_stmt_types(&body), vec!["log", "run_shell_cmd"]);
    }

    #[test]
    fn test_fn_body_orders() {
        let body = parse_fn_body(
            "pr t {\
              fn deploy {\
                env { CGO_ENABLED = (0); } {\
                  $(deploy);\
                };\
                cd (./dist);\
                $(npm publish);\
                log (done);\
              };\
            };",
        );
        assert_eq!(
            count_fn_stmt_types(&body),
            vec!["env", "cd", "run_shell_cmd", "log"]
        );
    }

    #[test]
    fn test_env_block_contents() {
        let body = parse_fn_body(
            "pr t {\
              fn test {\
                env { X = (1); } {\
                  $(run tests);\
                  log (testing);\
                };\
              };\
            };",
        );
        assert_eq!(body.len(), 1);
        let env = match &body[0] {
            FnStmt::EnvBlock { body, .. } => body,
            other => panic!("expected EnvBlock, got {:?}", other),
        };
        assert_eq!(env.len(), 2);
        assert!(matches!(&env[0], FnStmt::Bind { target: None, .. }));
        assert!(matches!(&env[1], FnStmt::Log(_)));
    }

    #[test]
    fn test_switch_branches() {
        let body = parse_fn_body(
            "pr t {\
              fn deploy {\
                switch @(target) {\
                  case (production) { $(deploy-prod); };\
                  case _ { log (unknown); };\
                };\
              };\
            };",
        );
        let sw = match &body[0] {
            FnStmt::Switch { arms, .. } => arms,
            other => panic!("expected Switch, got {:?}", other),
        };
        assert_eq!(sw.len(), 2);
        assert!(matches!(sw[0].pattern, ArmPattern::Lit(_)));
        assert!(matches!(sw[1].pattern, ArmPattern::Default));
    }

    #[test]
    fn test_case_pattern_must_be_literal() {
        let result = parse_program(
            "pr t {\
              fn d {\
                switch @(t) {\
                  case @(v) { log (x); };\
                };\
              };\
            };",
        );
        let errs = result.unwrap_err();
        assert!(
            errs.iter()
                .any(|e| e.to_string().contains("case pattern must be literal text or `_`")),
            "got: {:?}",
            errs
        );
    }

    #[test]
    fn test_case_command_pattern_rejected() {
        let result = parse_program(
            "pr t {\
              fn d {\
                switch @(t) {\
                  case $(cmd) { log (x); };\
                };\
              };\
            };",
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_cd_statement() {
        let body = parse_fn_body("pr t { fn build { cd (./src); }; };");
        assert_eq!(body.len(), 1);
        assert!(matches!(body[0], FnStmt::Cd(_)));
    }

    #[test]
    fn test_log_must_end_with_semicolon() {
        let result = parse_program("pr t { fn x { log (hi) }; };");
        assert!(result.is_err());
    }

    #[test]
    fn test_fn_decl_requires_semicolon() {
        let result = parse_program("pr t { fn x { log (hi); } };");
        assert!(result.is_err());
    }
}
