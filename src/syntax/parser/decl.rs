//! Top-level declaration parsers: `var`, `fn`, and project-level
//! `var`/`fn` inside `pr` bodies.

use super::*;

impl Parser {
    pub(crate) fn parse_var_decl(&mut self) -> Result<Stmt, ParseError> {
        let offset = self.current_token().offset;
        let len = self.current_token().len;
        let (name, value) = self.parse_var_decl_common()?;
        Ok(Stmt::Var {
            name,
            value,
            offset,
            len,
        })
    }

    pub(crate) fn parse_fn_decl(&mut self) -> Result<Stmt, ParseError> {
        let offset = self.current_token().offset;
        let len = self.current_token().len;
        self.advance();

        let name = self.parse_ident_name("function name")?;

        let body = self.parse_braced_block(
            "after function name",
            "to close function body",
            Self::parse_fn_stmt,
        )?;
        self.expect_with_context(TokenType::Semicolon, "after function declaration")?;

        Ok(Stmt::Fn {
            name,
            body,
            offset,
            len,
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::syntax::parser::test_support::*;
    use crate::syntax::{Stmt, TopLevel};

    #[test]
    fn test_var_decl() {
        let prog = parse_program("var x = (hello);").unwrap();
        match &prog.top_level_items[0] {
            TopLevel::Stmt(Stmt::Var { name, value, .. }) => {
                assert_eq!(name, "x");
                let text: String = value
                    .parts
                    .iter()
                    .map(|p| match p {
                        crate::syntax::source::Part::Lit(s) => s.clone(),
                        _ => String::new(),
                    })
                    .collect();
                assert_eq!(text, "hello");
            }
            _ => panic!("expected VarDecl"),
        }
    }

    #[test]
    fn test_var_missing_name() {
        let result = parse_program("var = (hello);");
        assert!(result.is_err());
    }

    #[test]
    fn test_var_missing_value() {
        let result = parse_program("var x = ;");
        assert!(result.is_err());
    }

    #[test]
    fn test_missing_semicolon() {
        let result = parse_program("var x = (hello)");
        assert!(result.is_err());
        let errs = result.unwrap_err();
        assert!(errs.iter().any(|e| e.to_string().contains("expected")));
    }

    #[test]
    fn test_unclosed_fn_brace() {
        let result = parse_program("pr t { fn bad { log (hi); };");
        assert!(result.is_err());
    }

    #[test]
    fn test_toplevel_fn_is_rejected() {
        let result = parse_program("fn build { log (hi); };");
        assert!(result.is_err());
    }
}
