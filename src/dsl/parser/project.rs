use super::*;
use crate::dsl::ProjectField;
use std::str::FromStr;

impl Parser {
    pub(crate) fn parse_project_decl(&mut self) -> Result<Stmt, ParseError> {
        self.advance(); // skip 'pr' or 'sync'

        let name = self.parse_ident_name("project name")?;

        self.expect_with_context(TokenType::LBrace, "after project name")?;

        if self.is_field_start() {
            // `sync name { url=(); ... };` — fields only.
            let fields = self.parse_project_fields_section()?;
            self.expect_with_context(TokenType::RBrace, "to close project field list")?;
            if self.current_token().token_type == TokenType::Semicolon {
                self.advance();
            }
            Ok(Stmt::Project {
                name,
                fields,
                body: Vec::new(),
            })
        } else {
            // `pr name { var...; fn...; };` — body only.
            let body = self.parse_project_body()?;
            if self.current_token().token_type == TokenType::RBrace {
                self.advance();
            }
            if self.current_token().token_type == TokenType::Semicolon {
                self.advance();
            }
            Ok(Stmt::Project {
                name,
                fields: Vec::new(),
                body,
            })
        }
    }

    /// True when the current token begins a project field (`url`/`dir`/`branch`
    /// as an identifier, or `sync` as the keyword).
    fn is_field_start(&self) -> bool {
        match &self.current_token().token_type {
            TokenType::Sync => true,
            TokenType::Ident(s) => matches!(s.as_str(), "url" | "dir" | "branch"),
            _ => false,
        }
    }

    fn parse_field_key(&mut self) -> Result<ProjectField, ParseError> {
        match &self.current_token().token_type {
            TokenType::Sync => {
                self.advance();
                Ok(ProjectField::Sync)
            }
            TokenType::Ident(s) => {
                let key = s.clone();
                self.advance();
                ProjectField::from_str(&key).map_err(|_| {
                    ParseError::new(
                        self.eof_aware_span(),
                        format!("unknown project field: {}", key),
                    )
                })
            }
            _ => Err(ParseError::new(
                self.eof_aware_span(),
                "expected project field name".to_string(),
            )),
        }
    }

    fn parse_project_fields_section(&mut self) -> Result<Vec<Stmt>, ParseError> {
        let mut fields = Vec::new();
        let mut seen_fields: std::collections::HashSet<ProjectField> =
            std::collections::HashSet::new();
        while self.current_token().token_type != TokenType::RBrace {
            let type_offset = self.current_token().offset;
            let key = self.parse_field_key()?;

            if !seen_fields.insert(key) {
                return Err(ParseError::new(
                    self.eof_aware_span(),
                    format!("duplicate project field: {}", key.as_str()),
                ));
            }

            self.expect_with_context(TokenType::Assign, "in project field")?;

            let value = self.parse_expr()?;

            // A trailing `;` after a field is optional: fields may also be
            // separated by newlines, as in `sync name { url = (..) dir = (..) }`.
            if self.current_token().token_type == TokenType::Semicolon {
                self.advance();
            }

            let field_len = self.current_token().offset - type_offset;
            fields.push(Stmt::Field {
                key,
                value,
                offset: type_offset,
                len: field_len,
            });
        }
        Ok(fields)
    }

    fn parse_project_body(&mut self) -> Result<Vec<Stmt>, ParseError> {
        let mut body = Vec::new();
        while self.current_token().token_type != TokenType::RBrace {
            match &self.current_token().token_type {
                TokenType::Var => {
                    body.push(self.parse_var_decl()?);
                }
                TokenType::Fn => {
                    body.push(self.parse_fn_decl()?);
                }
                _ => {
                    return Err(ParseError::new(
                        self.eof_aware_span(),
                        "expected `var` or `fn` in project body".to_string(),
                    ));
                }
            }
        }
        Ok(body)
    }
}

#[cfg(test)]
mod tests {
    use crate::dsl::parser::test_support::*;
    use crate::dsl::{Stmt, TopLevel};

    #[test]
    fn test_sync_with_fields() {
        let input = "sync myproj { url = (u); dir = (d); branch = (b); sync = (ignore); };";
        let prog = parse_program(input).unwrap();
        assert_eq!(count_stmt_types(&prog), vec!["pr"]);
        match &prog.top_level_items[0] {
            TopLevel::Stmt(Stmt::Project {
                name, fields, body, ..
            }) => {
                assert_eq!(name, "myproj");
                assert_eq!(fields.len(), 4);
                assert!(body.is_empty());
            }
            _ => panic!("expected Project"),
        }
    }

    #[test]
    fn test_pr_with_body() {
        let input = "pr p { var app = (todo); fn build { log (x); } };";
        let prog = parse_program(input).unwrap();
        match &prog.top_level_items[0] {
            TopLevel::Stmt(Stmt::Project { fields, body, .. }) => {
                assert!(fields.is_empty());
                assert_eq!(count_body_stmt_types(body), vec!["var", "fn"]);
            }
            _ => panic!("expected Project"),
        }
    }

    #[test]
    fn test_project_unknown_field_key_errors() {
        let result = parse_program("sync p { unknown = (v); };");
        assert!(result.is_err());
    }

    #[test]
    fn test_project_duplicate_field_errors() {
        let result = parse_program("sync p { url = (a); url = (b); };");
        assert!(result.is_err());
    }

    #[test]
    fn test_project_missing_field_value() {
        let result = parse_program("sync p { url = };");
        assert!(result.is_err());
    }
}
