//! Function body parser: `log`, `cd`, `var`, `env`, `switch`, and
//! bare `$(cmd)` statements inside `fn { ... }` blocks.

use super::*;
use crate::syntax::fnstmt::{Arm, FnStmt};
use crate::syntax::source::{ArmPattern, EnvPair};

impl Parser {
    /// Parses an `exec(cmd);` statement. `$()`/`@()` inside the argument
    /// are substituted at runtime; the resolved text is the command. An
    /// empty argument list is rejected: there is nothing to run.
    pub(crate) fn parse_exec_stmt(&mut self) -> Result<FnStmt, ParseError> {
        let args = match &self.current_token().token_type {
            TokenType::Exec(Some(args)) => args.clone(),
            _ => unreachable!("exec dispatch guarantees a fused argument list"),
        };
        if args.is_empty() {
            return Err(ParseError::new(
                self.eof_aware_span(),
                "`exec` requires a command".to_string(),
            ));
        }
        let command = self.single_argument(args, "exec")?;
        self.advance();
        self.expect_with_context(TokenType::Semicolon, "after `exec`")?;
        Ok(FnStmt::Exec(command))
    }

    /// Parses a `name(args);` call statement. The argument list is fused by
    /// the lexer; the target resolves at compile time by name among the
    /// global functions, with arguments bound to its params.
    pub(crate) fn parse_call_stmt(&mut self) -> Result<FnStmt, ParseError> {
        let (name, args, offset, len) = match &self.current_token().token_type {
            TokenType::Call { name, args } => (
                name.clone(),
                args.clone(),
                self.current_token().offset,
                self.current_token().len,
            ),
            _ => unreachable!("call dispatch guarantees a fused call token"),
        };
        self.advance();
        self.expect_with_context(TokenType::Semicolon, "after function call")?;
        Ok(FnStmt::Call {
            name,
            args,
            offset,
            len,
        })
    }

    /// Parses `project(name) { ... };`. The name argument is a template:
    /// `project(app)` is compile-time literal, `project($(cat .project))`
    /// resolves at runtime. Inside the body, every call runs in that
    /// project's context.
    pub(crate) fn parse_project_block(&mut self) -> Result<FnStmt, ParseError> {
        let args = match &self.current_token().token_type {
            TokenType::Project(Some(args)) => args.clone(),
            _ => unreachable!("project dispatch guarantees a fused argument list"),
        };
        if args.is_empty() {
            return Err(ParseError::new(
                self.eof_aware_span(),
                "`project` requires a project name".to_string(),
            ));
        }
        let name = self.single_argument(args, "project")?;
        self.advance();
        let body = self.parse_braced_block(
            "after `project(name)`",
            "to close project block",
            Self::parse_fn_stmt,
        )?;
        self.expect_with_context(TokenType::Semicolon, "after project block")?;
        Ok(FnStmt::Project { name, body })
    }

    /// Parses `async() { ... };` - a concurrent body joined at the end of
    /// the enclosing body.
    pub(crate) fn parse_async_block(&mut self) -> Result<FnStmt, ParseError> {
        let args = match &self.current_token().token_type {
            TokenType::Async(Some(args)) => args.clone(),
            _ => unreachable!("async dispatch guarantees a fused argument list"),
        };
        if !args.is_empty() {
            return Err(ParseError::new(
                self.eof_aware_span(),
                "`async` takes no arguments".to_string(),
            ));
        }
        self.advance();
        let body = self.parse_braced_block(
            "after `async()`",
            "to close async body",
            Self::parse_fn_stmt,
        )?;
        self.expect_with_context(TokenType::Semicolon, "after async block")?;
        Ok(FnStmt::Async { body })
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
            TokenType::Switch(Some(args)) => self.single_argument(args.clone(), "switch")?,
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
            TokenType::Case(Some(args)) => {
                // The pattern is validated as literal-only at compile time,
                // after `@()` references are inlined: `case(@(x))` compares
                // against x's value, while `$(cmd)` has no static text.
                let pattern_template = self.single_argument(args.clone(), "case")?;
                self.advance();
                ArmPattern::Template(pattern_template)
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

    /// Parse a top-level function body: functions are the named bundles the
    /// grammar declares at the top level.
    fn parse_fn_body(input: &str) -> Vec<FnStmt> {
        let prog = parse_program(input).unwrap();
        match &prog.top_level_items[0] {
            crate::syntax::TopLevel::Stmt(Stmt::Fn { body, .. }) => body.clone(),
            other => panic!("expected fn, got {:?}", other),
        }
    }

    #[test]
    fn test_fn_body_with_log_exec() {
        let body = parse_fn_body(
            "fn build {\
                log(compiling);\
                exec(cargo build);\
            };",
        );
        assert_eq!(count_fn_stmt_types(&body), vec!["log", "exec"]);
    }

    #[test]
    fn test_fn_body_orders() {
        let body = parse_fn_body(
            "fn deploy {\
                env(CGO_ENABLED = (0);) {\
                  exec(deploy);\
                };\
                cd(./dist);\
                exec(npm publish);\
                log(done);\
            };",
        );
        assert_eq!(count_fn_stmt_types(&body), vec!["env", "cd", "exec", "log"]);
    }

    #[test]
    fn test_env_block_contents() {
        let body = parse_fn_body(
            "fn test {\
                env(X = (1);) {\
                  exec(run tests);\
                  log(testing);\
                };\
            };",
        );
        assert_eq!(body.len(), 1);
        let env = match &body[0] {
            FnStmt::EnvBlock { body, .. } => body,
            other => panic!("expected EnvBlock, got {:?}", other),
        };
        assert_eq!(env.len(), 2);
        assert!(matches!(&env[0], FnStmt::Exec(_)));
        assert!(matches!(&env[1], FnStmt::Log(_)));
    }

    #[test]
    fn test_switch_branches() {
        let body = parse_fn_body(
            "fn deploy {\
                switch(@(target)) {\
                  case(production) { exec(deploy-prod); };\
                  default { log(unknown); };\
                };\
            };",
        );
        let sw = match &body[0] {
            FnStmt::Switch { arms, .. } => arms,
            other => panic!("expected Switch, got {:?}", other),
        };
        assert_eq!(sw.len(), 2);
        assert!(matches!(sw[0].pattern, ArmPattern::Template(_)));
        assert!(matches!(sw[1].pattern, ArmPattern::Default));
    }

    #[test]
    fn test_cd_statement() {
        let body = parse_fn_body("fn build { cd(./src); };");
        assert_eq!(body.len(), 1);
        assert!(matches!(body[0], FnStmt::Cd(_)));
    }

    #[test]
    fn test_calls_carry_positional_arguments() {
        let body = parse_fn_body("fn build { deploy(app-name; $(git rev-parse HEAD)); };");
        match &body[0] {
            FnStmt::Call { name, args, .. } => {
                assert_eq!(name, "deploy");
                assert_eq!(args.len(), 2, "args are `;`-separated templates");
                assert_eq!(args[0].literal_text(), "app-name");
            }
            other => panic!("expected Call, got {:?}", other),
        }
    }

    #[test]
    fn test_zero_argument_call_has_no_args() {
        let body = parse_fn_body("fn build { deploy(); };");
        match &body[0] {
            FnStmt::Call { name, args, .. } => {
                assert_eq!(name, "deploy");
                assert!(args.is_empty());
            }
            other => panic!("expected Call, got {:?}", other),
        }
    }

    #[test]
    fn test_exec_requires_a_command() {
        let result = parse_program("fn x { exec(); };");
        let errs = result.unwrap_err();
        assert!(
            errs.iter()
                .any(|e| e.to_string().contains("`exec` requires a command")),
            "got: {:?}",
            errs
        );
    }

    #[test]
    fn test_log_must_end_with_semicolon() {
        let result = parse_program("fn x { log(hi) };");
        assert!(result.is_err());
    }

    #[test]
    fn test_fn_decl_requires_semicolon() {
        let result = parse_program("fn x { log(hi); }");
        assert!(result.is_err());
    }
}
