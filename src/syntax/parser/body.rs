//! Function body parser: `log`, `cd`, `var`, `env`, `switch`, and
//! bare `$(cmd)` statements inside `fn { ... }` blocks.

use super::*;
use crate::syntax::fnstmt::{Arm, FnStmt};
use crate::syntax::source::{ArmPattern, EnvPair};

impl Parser {
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
        Ok(FnStmt::RunShellCmd(value))
    }

    /// Parses a `name();` call statement. The parens are fused by the lexer
    /// and must be empty: calls take no arguments, the target resolves at
    /// compile time against the enclosing project and the global functions.
    pub(crate) fn parse_call_stmt(&mut self) -> Result<FnStmt, ParseError> {
        let (name, template, offset, len) = match &self.current_token().token_type {
            TokenType::Call { name, template } => (
                name.clone(),
                template.clone(),
                self.current_token().offset,
                self.current_token().len,
            ),
            _ => unreachable!("call dispatch guarantees a fused call token"),
        };
        if !template.parts.is_empty() {
            return Err(ParseError::new(
                crate::diagnostics::Span::new(template.offset, template.len.max(1)),
                format!("function call `{name}` takes no arguments"),
            ));
        }
        self.advance();
        self.expect_with_context(TokenType::Semicolon, "after function call")?;
        Ok(FnStmt::Call { name, offset, len })
    }

    /// Parses `env(pairs) { body };`. The current token is the fused `env(`
    /// opener; pairs are an ordinary token stream (`KEY = template;` separated
    /// by `;`) until the bare closing `)`.
    pub(crate) fn parse_env_block(&mut self) -> Result<FnStmt, ParseError> {
        self.advance(); // past `env(`

        let mut pairs = Vec::new();
        loop {
            if matches!(self.current_token().token_type, TokenType::RParen) {
                self.advance();
                break;
            }
            let key = self.parse_ident_name("identifier in env pair")?;
            self.expect_with_context(TokenType::Assign, "in env pair")?;
            let value = self.parse_expr()?;
            pairs.push(EnvPair { key, value });
            match self.current_token().token_type {
                TokenType::Semicolon => self.advance(),
                TokenType::RParen => {}
                _ => {
                    return Err(ParseError::new(
                        self.eof_aware_span(),
                        format!(
                            "expected `;` or `)` after env pair, found {}",
                            format_token(self.current_token())
                        ),
                    ));
                }
            }
        }

        // The body executed with those env vars exported.
        let body = self.parse_braced_block(
            "after env pairs",
            "to close env block body",
            Self::parse_fn_stmt,
        )?;

        self.expect_with_context(TokenType::Semicolon, "after env block")?;

        Ok(FnStmt::EnvBlock { pairs, body })
    }

    /// Parses a `switch(subject) { case(pat) { ... } default { ... } };`
    /// block. The subject template is fused into the `switch(...)` token;
    /// each arm is a call: `case(pattern)` or the wildcard `default()`.
    pub(crate) fn parse_switch_stmt(&mut self) -> Result<FnStmt, ParseError> {
        let subject = match &self.current_token().token_type {
            TokenType::Switch(Some(subject)) => subject.clone(),
            _ => unreachable!("switch dispatch guarantees a fused subject"),
        };
        self.advance();

        self.expect_with_context(TokenType::LBrace, "to open switch arms")?;

        let mut arms = Vec::new();
        while self.current_token().token_type != TokenType::RBrace {
            arms.push(self.parse_switch_arm()?);
        }
        self.expect_with_context(TokenType::RBrace, "to close switch block")?;

        self.expect_with_context(TokenType::Semicolon, "after switch block")?;

        Ok(FnStmt::Switch { subject, arms })
    }

    /// Parses one switch arm: `case(pattern) { body };` or the bare wildcard
    /// `default { body };`.
    fn parse_switch_arm(&mut self) -> Result<Arm, ParseError> {
        let pattern = match &self.current_token().token_type {
            TokenType::Case(Some(pattern_template)) => {
                let pattern_template = pattern_template.clone();
                self.advance();
                // Patterns are matched against the resolved subject string,
                // so only literal text is meaningful: `@(var)` inlines to the
                // variable's value at compile time and `$(cmd)` has no static
                // text, so neither can form a pattern.
                let pattern_is_literal = pattern_template
                    .parts
                    .iter()
                    .all(|p| matches!(p, crate::syntax::source::Part::Lit(_)));
                if !pattern_is_literal {
                    return Err(ParseError::new(
                        crate::diagnostics::Span::new(
                            pattern_template.offset,
                            pattern_template.len.max(1),
                        ),
                        "case pattern must be literal text".to_string(),
                    ));
                }
                ArmPattern::Lit(pattern_template.literal_text())
            }
            TokenType::Default => {
                self.advance();
                ArmPattern::Default
            }
            _ => {
                return Err(self.unexpected_token_error());
            }
        };

        let body = self.parse_braced_block(
            "after switch pattern",
            "to close switch arm body",
            Self::parse_fn_stmt,
        )?;

        self.expect_with_context(TokenType::Semicolon, "after switch arm")?;

        Ok(Arm { pattern, body })
    }
}

#[cfg(test)]
mod tests {
    use crate::syntax::Stmt;
    use crate::syntax::fnstmt::FnStmt;
    use crate::syntax::parser::test_support::*;
    use crate::syntax::source::ArmPattern;

    /// Parse a wrapped function body: tests describe `fn` bodies, which the
    /// grammar only allows inside a `project` block.
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
            "project t {\
              fn build {\
                log(compiling);\
                $(cargo build);\
              };\
            };",
        );
        assert_eq!(count_fn_stmt_types(&body), vec!["log", "run_shell_cmd"]);
    }

    #[test]
    fn test_fn_body_orders() {
        let body = parse_fn_body(
            "project t {\
              fn deploy {\
                env(CGO_ENABLED = (0);) {\
                  $(deploy);\
                };\
                cd(./dist);\
                $(npm publish);\
                log(done);\
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
            "project t {\
              fn test {\
                env(X = (1);) {\
                  $(run tests);\
                  log(testing);\
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
        assert!(matches!(&env[0], FnStmt::RunShellCmd(_)));
        assert!(matches!(&env[1], FnStmt::Log(_)));
    }

    #[test]
    fn test_switch_branches() {
        let body = parse_fn_body(
            "project t {\
              fn deploy {\
                switch(@(target)) {\
                  case(production) { $(deploy-prod); };\
                  default { log(unknown); };\
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
            "project t {\
              fn d {\
                switch(@(t)) {\
                  case(@(v)) { log(x); };\
                };\
              };\
            };",
        );
        let errs = result.unwrap_err();
        assert!(
            errs.iter()
                .any(|e| e.to_string().contains("case pattern must be literal text")),
            "got: {:?}",
            errs
        );
    }

    #[test]
    fn test_case_command_pattern_rejected() {
        let result = parse_program(
            "project t {\
              fn d {\
                switch(@(t)) {\
                  case($(cmd)) { log(x); };\
                };\
              };\
            };",
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_cd_statement() {
        let body = parse_fn_body("project t { fn build { cd(./src); }; };");
        assert_eq!(body.len(), 1);
        assert!(matches!(body[0], FnStmt::Cd(_)));
    }

    #[test]
    fn test_log_must_end_with_semicolon() {
        let result = parse_program("project t { fn x { log(hi) }; };");
        assert!(result.is_err());
    }

    #[test]
    fn test_fn_decl_requires_semicolon() {
        let result = parse_program("project t { fn x { log(hi); } };");
        assert!(result.is_err());
    }
}
