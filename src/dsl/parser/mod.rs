use crate::dsl::ast::*;
use crate::dsl::error::{ParseError, format_token, format_token_type, is_keyword_token};
use crate::dsl::lexer::Lexer;
use crate::dsl::token::{Token, TokenType};
use miette::SourceSpan;

mod expr;
mod fn_body;
mod fn_run;
mod project;
mod stmts;

#[cfg(test)]
mod tests;

pub struct Parser {
    lexer: Lexer,
    current: Token,
    source_len: usize,
}

impl Parser {
    pub fn new(mut lexer: Lexer) -> Self {
        let source_len = lexer.source_len();
        let current = lexer.next_token();
        Parser {
            lexer,
            current,
            source_len,
        }
    }

    fn current_token(&self) -> &Token {
        &self.current
    }

    fn advance(&mut self) {
        self.current = self.lexer.next_token();
    }

    fn eof_aware_span(&self) -> SourceSpan {
        let tok = &self.current;
        if tok.len == 0 && tok.offset >= self.source_len && self.source_len > 0 {
            let start = self.source_len.saturating_sub(1);
            return SourceSpan::new(start.into(), 1);
        }
        let len = if tok.len == 0 {
            1.min(self.source_len.saturating_sub(tok.offset))
        } else {
            tok.len
        };
        SourceSpan::new(tok.offset.into(), len)
    }

    fn expect_with_context(&mut self, ty: TokenType, context: &str) -> Result<(), ParseError> {
        if self.current_token().ty == ty {
            self.advance();
            Ok(())
        } else {
            let token = self.current_token().clone();
            let expected = format_token_type(&ty);
            let found = format_token(&token);
            Err(ParseError::new(
                self.eof_aware_span(),
                format!("expected {} {}, found {}", expected, context, found),
            ))
        }
    }

    pub fn parse(&mut self) -> Result<Program, Vec<ParseError>> {
        let mut program = Program::new();
        let mut errors = Vec::new();

        while self.current_token().ty != TokenType::EOF {
            match self.parse_toplevel_stmt() {
                Ok(stmt) => program.stmts.push(stmt),
                Err(e) => {
                    errors.push(e);
                    self.skip_to_stmt_boundary();
                }
            }
        }

        if errors.is_empty() {
            Ok(program)
        } else {
            Err(errors)
        }
    }

    fn parse_toplevel_stmt(&mut self) -> Result<Stmt, ParseError> {
        if let TokenType::Illegal(m) = &self.current_token().ty {
            let token = self.current_token().clone();
            return Err(ParseError::new(
                SourceSpan::new(token.offset.into(), token.len),
                m.clone(),
            ));
        }
        match self.current_token().ty {
            TokenType::Shell => self.parse_shell_decl(),
            TokenType::Sanctuary => self.parse_sanctuary_decl(),
            TokenType::Import => self.parse_import_decl(),
            TokenType::Var => self.parse_var_decl(),
            TokenType::Pr => self.parse_project_decl(),
            TokenType::Fn => self.parse_fn_decl(),
            TokenType::Run => self.parse_run_decl(),
            _ => {
                let is_underscore = matches!(
                    &self.current_token().ty,
                    TokenType::Ident(s) if s == "_"
                );
                if is_underscore {
                    Err(ParseError::new(
                        self.eof_aware_span(),
                        "`_` is only valid as a case pattern".to_string(),
                    ))
                } else {
                    Err(ParseError::new(
                        self.eof_aware_span(),
                        format!(
                            "expected shell, sanctuary, import, var, pr, fn, or run, found {}",
                            format_token(self.current_token())
                        ),
                    ))
                }
            }
        }
    }

    pub(crate) fn parse_project_body_stmt(&mut self) -> Result<Stmt, ParseError> {
        if let TokenType::Illegal(m) = &self.current_token().ty {
            let token = self.current_token().clone();
            return Err(ParseError::new(
                SourceSpan::new(token.offset.into(), token.len),
                m.clone(),
            ));
        }
        match self.current_token().ty {
            TokenType::Var => self.parse_var_decl(),
            TokenType::Fn => self.parse_fn_decl(),
            TokenType::Run => self.parse_run_decl(),
            _ => {
                let is_underscore = matches!(
                    &self.current_token().ty,
                    TokenType::Ident(s) if s == "_"
                );
                if is_underscore {
                    Err(ParseError::new(
                        self.eof_aware_span(),
                        "`_` is only valid as a case pattern".to_string(),
                    ))
                } else {
                    Err(ParseError::new(
                        self.eof_aware_span(),
                        format!(
                            "expected shell, sanctuary, import, var, pr, fn, or run, found {}",
                            format_token(self.current_token())
                        ),
                    ))
                }
            }
        }
    }

    fn skip_to_stmt_boundary(&mut self) {
        use TokenType::*;
        loop {
            match &self.current_token().ty {
                EOF => break,
                Semicolon | RBrace => {
                    self.advance();
                }
                Shell | Sanctuary | Import | Var | Pr | Fn | Run => break,
                _ => self.advance(),
            }
        }
    }

    pub(crate) fn into_source(self) -> String {
        self.lexer.into_source()
    }

    pub(crate) fn parse_var_decl_common(&mut self) -> Result<(VarType, String, Expr), ParseError> {
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

        Ok((var_type, name, value))
    }

    pub(crate) fn parse_fn_stmt(&mut self) -> Result<FnStmt, ParseError> {
        match &self.current_token().ty {
            TokenType::Log => self.parse_log_stmt(),
            TokenType::Exec => self.parse_exec_stmt(),
            TokenType::Cd => self.parse_cd_stmt(),
            TokenType::Var => self.parse_fn_var_decl(),
            TokenType::Env => self.parse_env_block(),
            TokenType::Case => self.parse_case_stmt(),
            TokenType::Illegal(msg) => {
                let token = self.current_token().clone();
                Err(ParseError::new(
                    SourceSpan::new(token.offset.into(), token.len),
                    msg.clone(),
                ))
            }
            TokenType::Semicolon => Err(ParseError::new(
                self.eof_aware_span(),
                "unexpected `;` (empty statement)".to_string(),
            )),
            _ => {
                let is_underscore = matches!(
                    &self.current_token().ty,
                    TokenType::Ident(s) if s == "_"
                );
                if is_underscore {
                    Err(ParseError::new(
                        self.eof_aware_span(),
                        "`_` is only valid as a case pattern".to_string(),
                    ))
                } else {
                    Err(ParseError::new(
                        self.eof_aware_span(),
                        format!(
                            "expected log, exec, cd, var, env, or case, found {}",
                            format_token(self.current_token())
                        ),
                    ))
                }
            }
        }
    }
}
