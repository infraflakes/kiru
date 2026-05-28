use super::*;

impl Parser {
    pub(crate) fn parse_fn_decl(&mut self) -> Result<Stmt, ParseError> {
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

        Ok(Stmt::FnDecl { name, body })
    }

    pub(crate) fn parse_run_decl(&mut self) -> Result<Stmt, ParseError> {
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

        Ok(Stmt::RunDecl { name, chains })
    }

    fn parse_chain(&mut self) -> Result<Vec<String>, ParseError> {
        let mut fns = Vec::new();
        fns.push(self.parse_block_fn_name_in_run()?);

        while self.current_token().ty == TokenType::Arrow {
            self.advance();
            fns.push(self.parse_block_fn_name_in_run()?);
        }

        self.expect_with_context(TokenType::Semicolon, "after run chain")?;
        Ok(fns)
    }

    fn parse_block_fn_name_in_run(&mut self) -> Result<String, ParseError> {
        match &self.current_token().ty {
            TokenType::Ident(n) => {
                let name = n.clone();
                self.advance();
                Ok(name)
            }
            _ => Err(ParseError::new(
                self.eof_aware_span(),
                format!(
                    "expected function name in run block, found {}",
                    format_token(self.current_token())
                ),
            )),
        }
    }
}
