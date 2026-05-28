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

        let path = self.parse_expr()?;
        self.expect_with_context(TokenType::Semicolon, "after import path")?;

        Ok(Stmt::ImportDecl { path })
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
                    self.eof_aware_span(),
                    "expected 'string' or 'shell'".to_string(),
                ));
            }
        };

        let name = match &self.current_token().ty {
            TokenType::Ident(n) => n.clone(),
            ty if is_keyword_token(ty) => {
                return Err(ParseError::new(
                    self.eof_aware_span(),
                    format!(
                        "expected variable name, found {} (reserved keyword)",
                        format_token(self.current_token())
                    ),
                ));
            }
            _ => {
                return Err(ParseError::new(
                    self.eof_aware_span(),
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
