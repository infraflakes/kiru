use super::expr::parse_interpolation_parts;
use super::*;

impl Parser {
    pub(crate) fn parse_log_stmt(&mut self) -> Result<FnStmt, ParseError> {
        self.advance();

        let value = self.parse_expr()?;
        self.expect_with_context(TokenType::Semicolon, "after `log`")?;

        Ok(FnStmt::Log { value })
    }

    pub(crate) fn parse_exec_stmt(&mut self) -> Result<FnStmt, ParseError> {
        self.advance();

        let value = self.parse_expr()?;
        self.expect_with_context(TokenType::Semicolon, "after `exec`")?;

        Ok(FnStmt::Exec { value })
    }

    pub(crate) fn parse_cd_stmt(&mut self) -> Result<FnStmt, ParseError> {
        self.advance();

        let arg = self.parse_expr()?;
        self.expect_with_context(TokenType::Semicolon, "after `cd`")?;

        Ok(FnStmt::Cd { value: arg })
    }

    pub(crate) fn parse_fn_var_decl(&mut self) -> Result<FnStmt, ParseError> {
        let (var_type, name, value) = self.parse_var_decl_common()?;
        Ok(FnStmt::VarDecl {
            var_type,
            name,
            value,
        })
    }

    pub(crate) fn parse_env_block(&mut self) -> Result<FnStmt, ParseError> {
        self.advance();

        self.expect_with_context(TokenType::LBracket, "after `env`")?;

        let mut pairs = Vec::new();
        while self.current_token().ty != TokenType::RBracket {
            let key = match &self.current_token().ty {
                TokenType::Ident(key_str) => key_str.clone(),
                ty if is_keyword_token(ty) => {
                    return Err(ParseError::new(
                        self.eof_aware_span(),
                        format!(
                            "expected identifier in env pair, found {} (reserved keyword)",
                            format_token(self.current_token())
                        ),
                    ));
                }
                _ => {
                    return Err(ParseError::new(
                        self.eof_aware_span(),
                        "expected identifier in env pair".to_string(),
                    ));
                }
            };
            self.advance();

            self.expect_with_context(TokenType::Assign, "in env pair")?;

            let value = self.parse_expr()?;
            pairs.push(EnvPair { key, value });

            if self.current_token().ty == TokenType::RBracket {
                break;
            }
        }
        self.expect_with_context(TokenType::RBracket, "to close env pairs")?;

        self.expect_with_context(TokenType::LBrace, "to open env block body")?;

        let mut body = Vec::new();
        while self.current_token().ty != TokenType::RBrace {
            body.push(self.parse_fn_stmt()?);
        }
        self.expect_with_context(TokenType::RBrace, "to close env block body")?;

        self.expect_with_context(TokenType::Semicolon, "after env block")?;

        Ok(FnStmt::EnvBlock { pairs, body })
    }

    pub(crate) fn parse_case_stmt(&mut self) -> Result<FnStmt, ParseError> {
        self.advance();

        let condition = self.parse_expr()?;

        self.expect_with_context(TokenType::LBrace, "to open case arms")?;

        let mut scopes = Vec::new();
        while self.current_token().ty != TokenType::RBrace {
            let pattern = self.parse_case_pattern()?;

            self.expect_with_context(TokenType::LBrace, "after case pattern")?;
            let mut body = Vec::new();
            while self.current_token().ty != TokenType::RBrace {
                body.push(self.parse_fn_stmt()?);
            }
            self.expect_with_context(TokenType::RBrace, "to close case arm body")?;

            self.expect_with_context(TokenType::Semicolon, "after case arm")?;

            scopes.push(CaseArm { pattern, body });
        }
        self.expect_with_context(TokenType::RBrace, "to close case block")?;

        self.expect_with_context(TokenType::Semicolon, "after case block")?;

        Ok(FnStmt::Case { condition, scopes })
    }

    fn parse_case_pattern(&mut self) -> Result<CasePattern, ParseError> {
        let tok = self.current_token().clone();
        match &tok.ty {
            TokenType::Ident(ident) if ident == "_" => {
                self.advance();
                Ok(CasePattern::Default)
            }
            TokenType::Dollar => {
                let start_offset = self.current_token().offset;
                self.advance();
                let name = match &self.current_token().ty {
                    TokenType::Ident(name_str) => name_str.clone(),
                    ty if is_keyword_token(ty) => {
                        return Err(ParseError::new(
                            self.eof_aware_span(),
                            format!(
                                "expected identifier after `$` in case pattern, found {} (reserved keyword)",
                                format_token(self.current_token())
                            ),
                        ));
                    }
                    _ => {
                        return Err(ParseError::new(
                            self.eof_aware_span(),
                            "expected identifier after `$` in case pattern".to_string(),
                        ));
                    }
                };
                let end_offset = self.current_token().offset + self.current_token().len;
                self.advance();
                Ok(CasePattern::VarRef {
                    name,
                    offset: start_offset,
                    len: end_offset - start_offset,
                })
            }
            TokenType::Backtick(content) => {
                let offset = tok.offset;
                let len = tok.len;
                self.advance();
                let parts = parse_interpolation_parts(content, offset)?;
                Ok(CasePattern::Literal { parts, offset, len })
            }
            _ => {
                let token_str = format_token(&tok);
                Err(ParseError::new(
                    self.eof_aware_span(),
                    format!(
                        "expected pattern before {}; are you missing a case arm pattern?",
                        token_str
                    ),
                ))
            }
        }
    }
}
