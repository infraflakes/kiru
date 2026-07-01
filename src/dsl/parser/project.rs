use super::*;

impl Parser {
    pub(crate) fn parse_project_decl(&mut self) -> Result<Stmt, ParseError> {
        let offset = self.current_token().offset;
        let len = self.current_token().len;
        self.advance(); // skip 'pr'

        let name = match &self.current_token().ty {
            TokenType::Ident(name_str) => name_str.clone(),
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

        // `pr name [ field = value, ... ] { fn/run/var ... }`
        if self.current_token().ty == TokenType::LBracket {
            let fields = self.parse_project_fields_section()?;
            self.expect_with_context(TokenType::LBrace, "after project field list")?;
            let body = self.parse_project_body()?;
            return Ok(Stmt::Project {
                name,
                fields,
                body,
                offset,
                len,
            });
        }

        // `pr name { fn/run/var ... }` — body only, no fields
        self.expect_with_context(TokenType::LBrace, "after project name")?;
        let body = self.parse_project_body()?;

        Ok(Stmt::Project {
            name,
            fields: Vec::new(),
            body,
            offset,
            len,
        })
    }

    fn parse_project_fields_section(&mut self) -> Result<Vec<Stmt>, ParseError> {
        self.advance(); // skip '['

        let mut fields = Vec::new();
        while self.current_token().ty != TokenType::RBracket {
            let type_offset = self.current_token().offset;
            let key_str = match &self.current_token().ty {
                TokenType::Ident(ident) => ident.clone(),
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
                        "expected field name in project field list".to_string(),
                    ));
                }
            };
            self.advance();

            let key = match key_str.as_str() {
                "url" => ProjectField::Url,
                "dir" => ProjectField::Dir,
                "sync" => ProjectField::Sync,
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

            let field_len = self.current_token().offset - type_offset;
            fields.push(Stmt::Field {
                key,
                value,
                offset: type_offset,
                len: field_len,
            });
        }

        self.expect_with_context(TokenType::RBracket, "to close project field list")?;
        Ok(fields)
    }

    fn parse_project_body(&mut self) -> Result<Vec<Stmt>, ParseError> {
        let mut body = Vec::new();
        while self.current_token().ty != TokenType::RBrace {
            match &self.current_token().ty {
                TokenType::Var | TokenType::Fn | TokenType::Run => {
                    body.push(self.parse_project_body_stmt()?);
                }
                _ => {
                    return Err(ParseError::new(
                        self.eof_aware_span(),
                        "expected fn, run, or var in project body".to_string(),
                    ));
                }
            }
        }
        self.expect_with_context(TokenType::RBrace, "to close project body")?;
        Ok(body)
    }
}
