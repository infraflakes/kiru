use super::expr::parse_template_parts;
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

        Ok(FnStmt::Cd { arg })
    }

    pub(crate) fn parse_fn_var_decl(&mut self) -> Result<FnStmt, ParseError> {
        self.advance();

        let var_type = match &self.current_token().ty {
            TokenType::StringKw => {
                self.advance();
                VarType::String
            }
            TokenType::Shell => {
                self.advance();
                VarType::Shell
            }
            _ => {
                return Err(ParseError::new(
                    self.eof_aware_span(),
                    "expected 'string' or 'shell'".to_string(),
                ));
            }
        };

        let name = match &self.current_token().ty {
            TokenType::Ident(n) => n.clone(),
            ty if is_keyword_token(ty) => {
                return Err(ParseError::new(
                    self.eof_aware_span(),
                    format!(
                        "expected variable name, found {} (reserved keyword)",
                        format_token(self.current_token())
                    ),
                ));
            }
            _ => {
                return Err(ParseError::new(
                    self.eof_aware_span(),
                    "expected variable name".to_string(),
                ));
            }
        };
        self.advance();

        self.expect_with_context(TokenType::Assign, "in variable declaration")?;

        let value = self.parse_expr()?;
        self.expect_with_context(TokenType::Semicolon, "after variable declaration")?;

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
                TokenType::Ident(k) => k.clone(),
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

            match &self.current_token().ty {
                TokenType::Comma => {
                    self.advance();
                }
                TokenType::RBracket => break,
                _ => {
                    return Err(ParseError::new(
                        self.eof_aware_span(),
                        "expected `,` or `]`".to_string(),
                    ));
                }
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
        self.advance(); // skip 'case'

        let condition = self.parse_expr()?;

        self.expect_with_context(TokenType::LBrace, "to open case arms")?;

        let mut arms = Vec::new();
        while self.current_token().ty != TokenType::RBrace {
            let pattern = self.parse_case_pattern()?;

            self.expect_with_context(TokenType::LBrace, "after case pattern")?;
            let mut body = Vec::new();
            while self.current_token().ty != TokenType::RBrace {
                body.push(self.parse_fn_stmt()?);
            }
            self.expect_with_context(TokenType::RBrace, "to close case arm body")?;

            self.expect_with_context(TokenType::Semicolon, "after case arm")?;

            arms.push(CaseArm { pattern, body });
        }
        self.expect_with_context(TokenType::RBrace, "to close case block")?;

        self.expect_with_context(TokenType::Semicolon, "after case block")?;

        Ok(FnStmt::Case { condition, arms })
    }

    fn parse_case_pattern(&mut self) -> Result<CasePattern, ParseError> {
        match &self.current_token().ty {
            TokenType::Ident(s) if s == "_" => {
                self.advance();
                Ok(CasePattern::Default)
            }
            TokenType::Dollar => {
                self.advance();
                let name = match &self.current_token().ty {
                    TokenType::Ident(n) => n.clone(),
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
                self.advance();
                Ok(CasePattern::VarRef { name })
            }
            TokenType::Backtick(_) => {
                let token = self.current_token().clone();
                self.advance();
                match &token.ty {
                    TokenType::Backtick(content) => {
                        let parts = parse_template_parts(content, token.offset)?;
                        Ok(CasePattern::Literal { parts })
                    }
                    _ => unreachable!(),
                }
            }
            _ => {
                let tok = format_token(self.current_token());
                Err(ParseError::new(
                    self.eof_aware_span(),
                    format!(
                        "expected pattern before {}; are you missing a case arm pattern?",
                        tok
                    ),
                ))
            }
        }
    }
}
