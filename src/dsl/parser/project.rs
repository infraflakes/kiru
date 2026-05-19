use super::*;

impl Parser {
    pub(crate) fn parse_project_decl(&mut self) -> Result<Stmt, ParseError> {
        self.advance(); // skip 'pr'

        let name = match &self.current_token().ty {
            TokenType::Ident(n) => n.clone(),
            _ => {
                return Err(ParseError::new(
                    miette::SourceSpan::new(
                        self.current_token().offset.into(),
                        self.current_token().len,
                    ),
                    "expected project name".to_string(),
                ));
            }
        };
        self.advance();

        self.expect_with_context(TokenType::LBrace, "after project name")?;

        let mut fields = Vec::new();
        let mut body = Vec::new();

        while self.current_token().ty != TokenType::RBrace {
            match &self.current_token().ty {
                TokenType::Var | TokenType::Fn | TokenType::Seq | TokenType::Par => {
                    body.push(self.parse_project_body_stmt()?);
                }
                _ => {
                    let key = match &self.current_token().ty {
                        TokenType::Ident(k) => k.clone(),
                        _ => {
                            return Err(ParseError::new(
                                miette::SourceSpan::new(
                                    self.current_token().offset.into(),
                                    self.current_token().len,
                                ),
                                "expected field name or var/fn/seq/par".to_string(),
                            ));
                        }
                    };
                    self.advance();

                    self.expect_with_context(TokenType::Assign, "in project field")?;

                    let value = self.parse_expr()?;
                    self.expect_with_context(TokenType::Semicolon, "after project field value")?;

                    fields.push(ProjectField { key, value });
                }
            }
        }

        self.expect_with_context(TokenType::RBrace, "to close project body")?;

        Ok(Stmt::ProjectDecl { name, fields, body })
    }
}
