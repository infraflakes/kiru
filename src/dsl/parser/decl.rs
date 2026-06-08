use super::*;

impl Parser {
    pub(crate) fn parse_shell_decl(&mut self) -> Result<Stmt, ParseError> {
        let offset = self.current_token().offset;
        let len = self.current_token().len;
        self.advance();

        self.expect_with_context(TokenType::Assign, "after `shell`")?;

        let value = self.parse_simple_backtick()?;
        self.expect_with_context(TokenType::Semicolon, "after shell declaration")?;

        Ok(Stmt::ShellDecl { value, offset, len })
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
        let offset = self.current_token().offset;
        let len = self.current_token().len;
        let (var_type, name, value) = self.parse_var_decl_common()?;
        Ok(Stmt::VarDecl {
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

        let name = match &self.current_token().ty {
            TokenType::Ident(n) => n.clone(),
            ty if is_keyword_token(ty) => {
                return Err(ParseError::new(
                    self.eof_aware_span(),
                    format!(
                        "expected function name, found {} (reserved keyword)",
                        format_token(self.current_token())
                    ),
                ));
            }
            _ => {
                return Err(ParseError::new(
                    self.eof_aware_span(),
                    "expected function name".to_string(),
                ));
            }
        };
        self.advance();

        self.expect_with_context(TokenType::LBrace, "after function name")?;

        let mut body = Vec::new();
        while self.current_token().ty != TokenType::RBrace {
            body.push(self.parse_fn_stmt()?);
        }

        self.expect_with_context(TokenType::RBrace, "to close function body")?;

        Ok(Stmt::FnDecl {
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

        let name = match &self.current_token().ty {
            TokenType::Ident(n) => n.clone(),
            ty if is_keyword_token(ty) => {
                return Err(ParseError::new(
                    self.eof_aware_span(),
                    format!(
                        "expected run block name, found {} (reserved keyword)",
                        format_token(self.current_token())
                    ),
                ));
            }
            _ => {
                return Err(ParseError::new(
                    self.eof_aware_span(),
                    "expected run block name".to_string(),
                ));
            }
        };
        self.advance();

        self.expect_with_context(TokenType::LBrace, "after run block name")?;

        let mut chains = Vec::new();
        while self.current_token().ty != TokenType::RBrace {
            if self.current_token().ty == TokenType::EOF {
                return Err(ParseError::new(
                    self.eof_aware_span(),
                    "unexpected end of file in run declaration (expected '}')".to_string(),
                ));
            }
            chains.push(self.parse_chain()?);
        }

        self.expect_with_context(TokenType::RBrace, "to close run block body")?;

        Ok(Stmt::RunDecl {
            name,
            chains,
            offset,
            len,
        })
    }
}
