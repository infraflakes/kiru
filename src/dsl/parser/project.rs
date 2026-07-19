use super::*;

impl Parser {
    pub(crate) fn parse_project_decl(&mut self) -> Result<Stmt, ParseError> {
        let offset = self.current_token().offset;
        let len = self.current_token().len;
        self.advance(); // skip 'pr'

        let name = self.parse_ident_name("project", "expected project name")?;

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
        let mut seen_fields: std::collections::HashSet<ProjectField> =
            std::collections::HashSet::new();
        while self.current_token().ty != TokenType::RBracket {
            let type_offset = self.current_token().offset;
            let key_str =
                self.parse_ident_name("field", "expected field name in project field list")?;

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

            if !seen_fields.insert(key.clone()) {
                return Err(ParseError::new(
                    self.eof_aware_span(),
                    format!("duplicate project field: {}", key_str),
                ));
            }

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
                TokenType::Var | TokenType::Fn => {
                    body.push(self.parse_project_body_stmt()?);
                }
                _ => {
                    return Err(ParseError::new(
                        self.eof_aware_span(),
                        "expected fn or var in project body".to_string(),
                    ));
                }
            }
        }
        self.expect_with_context(TokenType::RBrace, "to close project body")?;
        Ok(body)
    }
}

#[cfg(test)]
mod tests {
    use crate::dsl::parser::test_support::*;
    use crate::dsl::{ProjectField, Stmt, TopLevel};

    #[test]
    fn test_project_with_url_and_dir() {
        let input = "pr myproj [url = `u` dir = `d`] { fn build { exec `make`; } }";
        let prog = parse_program(input).unwrap();
        assert_eq!(count_stmt_types(&prog), vec!["pr"]);
        match &prog.items[0] {
            TopLevel::Stmt(Stmt::Project {
                name, fields, body, ..
            }) => {
                assert_eq!(name, "myproj");
                assert_eq!(fields.len(), 2);
                assert!(matches!(
                    &fields[0],
                    Stmt::Field {
                        key: ProjectField::Url,
                        ..
                    }
                ));
                assert!(matches!(
                    &fields[1],
                    Stmt::Field {
                        key: ProjectField::Dir,
                        ..
                    }
                ));
                assert_eq!(count_body_stmt_types(body), vec!["fn"]);
            }
            _ => panic!("expected Project"),
        }
    }

    #[test]
    fn test_project_with_all_fields() {
        let input = "pr p [url = `u` dir = `d` sync = `s` branch = `b`] { fn f { log `x`; } }";
        let prog = parse_program(input).unwrap();
        match &prog.items[0] {
            TopLevel::Stmt(Stmt::Project { fields, .. }) => {
                assert_eq!(fields.len(), 4);
            }
            _ => panic!("expected Project"),
        }
    }

    #[test]
    fn test_project_missing_opening_bracket() {
        let result = parse_program("pr p url = `u`] { }");
        assert!(result.is_err());
    }

    #[test]
    fn test_project_missing_closing_bracket() {
        let result = parse_program("pr p [url = `u` { }");
        assert!(result.is_err());
    }

    #[test]
    fn test_project_empty_fields() {
        let input = "pr p [] { fn b { log `x`; } }";
        let prog = parse_program(input).unwrap();
        match &prog.items[0] {
            TopLevel::Stmt(Stmt::Project { fields, body, .. }) => {
                assert!(fields.is_empty());
                assert_eq!(count_body_stmt_types(body), vec!["fn"]);
            }
            _ => panic!("expected Project"),
        }
    }

    #[test]
    fn test_project_without_fields_errors() {
        let result = parse_program("pr p { fn b { log `x`; }; }");
        assert!(result.is_err());
    }

    #[test]
    fn test_project_unknown_field_key_errors() {
        let result = parse_program("pr p [unknown = `v`] { }");
        assert!(result.is_err());
    }

    #[test]
    fn test_project_missing_field_value() {
        let result = parse_program("pr p [url = ] { }");
        assert!(result.is_err());
    }

    #[test]
    fn test_project_duplicate_field_errors() {
        let result = parse_program("pr p [url = `a` url = `b`] { }");
        assert!(result.is_err());
    }
}
