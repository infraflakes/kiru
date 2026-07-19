use super::*;

impl Parser {
    pub(crate) fn parse_var_decl(&mut self) -> Result<Stmt, ParseError> {
        let offset = self.current_token().offset;
        let len = self.current_token().len;
        let (var_type, name, value) = self.parse_var_decl_common()?;
        Ok(Stmt::Var {
            var_type,
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

        let name = self.parse_ident_name("function", "expected function name")?;

        self.expect_with_context(TokenType::LBrace, "after function name")?;

        let mut body = Vec::new();
        while self.current_token().ty != TokenType::RBrace {
            body.push(self.parse_fn_stmt()?);
        }

        self.expect_with_context(TokenType::RBrace, "to close function body")?;

        Ok(Stmt::Fn {
            name,
            body,
            offset,
            len,
        })
    }

    pub(crate) fn parse_run_decl(&mut self) -> Result<Stmt, ParseError> {
        let offset = self.current_token().offset;
        let len = self.current_token().len;
        self.advance();

        let name = self.parse_ident_name("run block", "expected run block name")?;

        self.expect_with_context(TokenType::LBrace, "after run block name")?;

        let mut chains = Vec::new();
        while self.current_token().ty != TokenType::RBrace {
            if self.current_token().ty == TokenType::Eof {
                return Err(ParseError::new(
                    self.eof_aware_span(),
                    "unexpected end of file in run declaration (expected '}')".to_string(),
                ));
            }
            chains.push(self.parse_chain()?);
        }

        self.expect_with_context(TokenType::RBrace, "to close run block body")?;

        Ok(Stmt::Run {
            name,
            chains,
            offset,
            len,
        })
    }
}

#[cfg(test)]
mod tests {
    use crate::dsl::parser::test_support::*;
    use crate::dsl::{Expr, Stmt, TopLevel, VarType};

    #[test]
    fn test_var_string_decl() {
        let prog = parse_program("var string x = `hello`;").unwrap();
        match &prog.items[0] {
            TopLevel::Stmt(Stmt::Var {
                var_type,
                name,
                value,
                ..
            }) => {
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
        match &prog.items[0] {
            TopLevel::Stmt(Stmt::Var { var_type, name, .. }) => {
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
    fn test_var_with_var_ref_value() {
        let input = "var string x = `a`; var string y = `${global::x}`;";
        let prog = parse_program(input).unwrap();
        assert_eq!(count_stmt_types(&prog), vec!["var", "var"]);
    }

    #[test]
    fn test_missing_semicolon() {
        let result = parse_program("var string x = `hello`");
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
    fn test_unclosed_fn_brace() {
        let result = parse_program("fn bad { log `hi`;");
        assert!(result.is_err());
    }

    #[test]
    fn test_unclosed_run_brace() {
        let result = parse_program("run s { check;");
        assert!(result.is_err());
    }
}
