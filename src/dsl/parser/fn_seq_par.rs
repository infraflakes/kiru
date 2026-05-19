use super::*;

impl Parser {
    pub(crate) fn parse_fn_decl(&mut self) -> Result<Stmt, ParseError> {
        self.advance();

        let name = match &self.current_token().ty {
            TokenType::Ident(n) => n.clone(),
            _ => {
                return Err(ParseError::new(
                    miette::SourceSpan::new(
                        self.current_token().offset.into(),
                        self.current_token().len,
                    ),
                    "expected identifier".to_string(),
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

    pub(crate) fn parse_seq_decl(&mut self) -> Result<Stmt, ParseError> {
        self.advance();

        let name = match &self.current_token().ty {
            TokenType::Ident(n) => n.clone(),
            _ => {
                return Err(ParseError::new(
                    miette::SourceSpan::new(
                        self.current_token().offset.into(),
                        self.current_token().len,
                    ),
                    "expected sequence name".to_string(),
                ));
            }
        };
        self.advance();

        self.expect_with_context(TokenType::LBrace, "after sequence name")?;

        let mut fns = Vec::new();
        while self.current_token().ty != TokenType::RBrace {
            if self.current_token().ty == TokenType::EOF {
                return Err(ParseError::new(
                    miette::SourceSpan::new(self.current_token().offset.into(), 0),
                    "unexpected end of file in seq declaration (expected '}')".to_string(),
                ));
            }
            fns.push(self.parse_block_fn_name()?);
        }

        self.expect_with_context(TokenType::RBrace, "to close sequence body")?;

        Ok(Stmt::SeqDecl { name, fns })
    }

    pub(crate) fn parse_par_decl(&mut self) -> Result<Stmt, ParseError> {
        self.advance();

        let name = match &self.current_token().ty {
            TokenType::Ident(n) => n.clone(),
            _ => {
                return Err(ParseError::new(
                    miette::SourceSpan::new(
                        self.current_token().offset.into(),
                        self.current_token().len,
                    ),
                    "expected parallel block name".to_string(),
                ));
            }
        };
        self.advance();

        self.expect_with_context(TokenType::LBrace, "after parallel block name")?;

        let mut fns = Vec::new();
        while self.current_token().ty != TokenType::RBrace {
            if self.current_token().ty == TokenType::EOF {
                return Err(ParseError::new(
                    miette::SourceSpan::new(self.current_token().offset.into(), 0),
                    "unexpected end of file in par declaration (expected '}')".to_string(),
                ));
            }
            fns.push(self.parse_block_fn_name()?);
        }

        self.expect_with_context(TokenType::RBrace, "to close parallel block body")?;

        Ok(Stmt::ParDecl { name, fns })
    }
}
