use super::*;

impl Parser {
    pub(crate) fn parse_project_decl(&mut self) -> Result<Stmt, ParseError> {
        let offset = self.current_token().offset;
        let len = self.current_token().len;
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

        let mut body = Vec::new();

        while self.current_token().ty != TokenType::RBrace {
            match &self.current_token().ty {
                TokenType::Var | TokenType::Fn | TokenType::Run => {
                    body.push(self.parse_project_body_stmt()?);
                }
                _ => {
                    let type_offset = self.current_token().offset;
                    let key_str = match &self.current_token().ty {
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

                    let key = match key_str.as_str() {
                        "url" => ProjectField::Url,
                        "dir" => ProjectField::Dir,
                        "sync" => ProjectField::Sync,
                        "include" => ProjectField::Include,
                        "branch" => ProjectField::Branch,
                        _ => {
                            return Err(ParseError::new(
                                self.eof_aware_span(),
                                format!("unknown project field: {}", key_str),
                            ));
                        }
                    };

                    self.expect_with_context(TokenType::Assign, "in project field")?;

                    let value = self.parse_expr()?;
                    self.expect_with_context(TokenType::Semicolon, "after project field value")?;

                    let field_len = self.current_token().offset - type_offset;
                    body.push(Stmt::Field {
                        key,
                        value,
                        offset: type_offset,
                        len: field_len,
                    });
                }
            }
        }

        self.expect_with_context(TokenType::RBrace, "to close project body")?;

        Ok(Stmt::Project {
            name,
            body,
            offset,
            len,
        })
    }
}
