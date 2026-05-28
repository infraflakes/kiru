use super::*;

impl Parser {
    pub(crate) fn parse_project_decl(&mut self) -> Result<Stmt, ParseError> {
        self.advance(); // skip 'pr'

        let name = match &self.current_token().ty {
            TokenType::Ident(n) => n.clone(),
            ty if is_keyword_token(ty) => {
                return Err(ParseError::new(
                    self.eof_aware_span(),
                    format!(
                        "expected project name, found {} (reserved keyword)",
                        format_token(self.current_token())
                    ),
                ));
            }
            _ => {
                return Err(ParseError::new(
                    self.eof_aware_span(),
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
                TokenType::Var | TokenType::Fn | TokenType::Run => {
                    body.push(self.parse_project_body_stmt()?);
                }
                _ => {
                    let key = match &self.current_token().ty {
                        TokenType::Ident(k) => k.clone(),
                        ty if is_keyword_token(ty) => {
                            return Err(ParseError::new(
                                self.eof_aware_span(),
                                format!(
                                    "expected field name, found {} (reserved keyword)",
                                    format_token(self.current_token())
                                ),
                            ));
                        }
                        _ => {
                            return Err(ParseError::new(
                                self.eof_aware_span(),
                                "expected field name or var/fn/run".to_string(),
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
