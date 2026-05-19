use super::*;

impl Parser {
    pub(crate) fn parse_shell_decl(&mut self) -> Result<Stmt, ParseError> {
        self.advance();

        self.expect_with_context(TokenType::Assign, "after `shell`")?;

        let value = self.parse_simple_backtick()?;
        self.expect_with_context(TokenType::Semicolon, "after shell declaration")?;

        Ok(Stmt::ShellDecl { value })
    }

    pub(crate) fn parse_sanctuary_decl(&mut self) -> Result<Stmt, ParseError> {
        self.advance();

        self.expect_with_context(TokenType::Assign, "after `sanctuary`")?;

        let value = self.parse_expr()?;
        self.expect_with_context(TokenType::Semicolon, "after sanctuary declaration")?;

        Ok(Stmt::SanctuaryDecl { value })
    }

    pub(crate) fn parse_import_decl(&mut self) -> Result<Stmt, ParseError> {
        self.advance();

        let path = match &self.current_token().ty {
            TokenType::PathLit(p) => p.clone(),
            _ => {
                return Err(ParseError::new(
                    miette::SourceSpan::new(
                        self.current_token().offset.into(),
                        self.current_token().len,
                    ),
                    "expected import path".to_string(),
                ));
            }
        };
        self.advance();

        self.expect_with_context(TokenType::Semicolon, "after import path")?;

        Ok(Stmt::ImportDecl { paths: vec![path] })
    }

    pub(crate) fn parse_var_decl(&mut self) -> Result<Stmt, ParseError> {
        self.advance();

        let var_type = match &self.current_token().ty {
            TokenType::StringKw => {
                self.advance();
                VarType::String
            }
            TokenType::Shell => {
                self.advance();
                VarType::Shell
            }
            _ => {
                return Err(ParseError::new(
                    miette::SourceSpan::new(
                        self.current_token().offset.into(),
                        self.current_token().len,
                    ),
                    "expected 'string' or 'shell'".to_string(),
                ));
            }
        };

        let name = match &self.current_token().ty {
            TokenType::Ident(n) => n.clone(),
            _ => {
                return Err(ParseError::new(
                    miette::SourceSpan::new(
                        self.current_token().offset.into(),
                        self.current_token().len,
                    ),
                    "expected variable name".to_string(),
                ));
            }
        };
        self.advance();

        self.expect_with_context(TokenType::Assign, "in variable declaration")?;

        let value = self.parse_expr()?;
        self.expect_with_context(TokenType::Semicolon, "after variable declaration")?;

        Ok(Stmt::VarDecl {
            var_type,
            name,
            value,
        })
    }
}
