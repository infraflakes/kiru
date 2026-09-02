//! Project parser: handles `pr name { ... }` declarations with function bodies.

use super::*;

impl Parser {
    pub(crate) fn parse_project_decl(&mut self) -> Result<Stmt, ParseError> {
        self.advance(); // skip 'pr'

        let name = self.parse_ident_name("project name")?;

        self.expect_with_context(TokenType::LBrace, "after project name")?;

        let body = self.parse_project_body()?;
        self.expect_with_context(TokenType::RBrace, "to close project body")?;
        self.expect_with_context(TokenType::Semicolon, "after project declaration")?;
        Ok(Stmt::Project { name, body })
    }

    fn parse_project_body(&mut self) -> Result<Vec<Stmt>, ParseError> {
        let mut body = Vec::new();
        while self.current_token().token_type != TokenType::RBrace {
            self.err_on_illegal_token()?;
            if self.current_token().token_type == TokenType::Eof {
                return Err(ParseError::new(
                    self.eof_aware_span(),
                    "expected `}` to close project body".to_string(),
                ));
            }
            match &self.current_token().token_type {
                TokenType::Var => {
                    body.push(self.parse_var_decl()?);
                }
                TokenType::Fn => {
                    body.push(self.parse_fn_decl()?);
                }
                _ => {
                    return Err(ParseError::new(
                        self.eof_aware_span(),
                        "expected `var` or `fn` in project body".to_string(),
                    ));
                }
            }
        }
        Ok(body)
    }
}

#[cfg(test)]
mod tests {
    use crate::syntax::parser::test_support::*;
    use crate::syntax::{Stmt, TopLevel};

    #[test]
    fn test_pr_with_body() {
        let input = "pr p { var app = (todo); fn build { log (x); }; };";
        let prog = parse_program(input).unwrap();
        match &prog.top_level_items[0] {
            TopLevel::Stmt(Stmt::Project { body, .. }) => {
                assert_eq!(count_body_stmt_types(body), vec!["var", "fn"]);
            }
            _ => panic!("expected Project"),
        }
    }

    #[test]
    fn test_pr_decl_requires_semicolon() {
        let result = parse_program("pr p { fn b { log (x); }; }");
        assert!(result.is_err());
    }

    #[test]
    fn test_pr_body_at_eof_reports_closing_brace() {
        let result = parse_program("pr p {");
        let errs = result.unwrap_err();
        assert!(
            errs.iter()
                .any(|e| e.to_string().contains("expected `}` to close project body")),
            "got: {:?}",
            errs
        );
    }

    #[test]
    fn test_sync_keyword_errors() {
        let result = parse_program("sync myproj { };");
        assert!(result.is_err());
    }
}
