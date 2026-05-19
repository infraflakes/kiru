use crate::dsl::ast::*;
use crate::dsl::lexer::Lexer;
use crate::dsl::token::{Token, TokenType};
use miette::{Diagnostic, SourceSpan};
use thiserror::Error;

mod block;
mod expr;
mod fn_body;
mod fn_seq_par;
mod globals;
mod project;

#[cfg(test)]
mod tests;

#[derive(Debug, Error, Diagnostic)]
#[error("{msg}")]
pub struct ParseError {
    #[label("{msg}")]
    span: SourceSpan,
    msg: String,
}

impl ParseError {
    fn new(span: SourceSpan, msg: String) -> Self {
        Self { span, msg }
    }
}

fn format_token_type(ty: &TokenType) -> &'static str {
    match ty {
        TokenType::LBrace => "`{`",
        TokenType::RBrace => "`}`",
        TokenType::LParen => "`(`",
        TokenType::RParen => "`)`",
        TokenType::LBracket => "`[`",
        TokenType::RBracket => "`]`",
        TokenType::Semicolon => "`;`",
        TokenType::Comma => "`,`",
        TokenType::Assign => "`=`",
        TokenType::Dollar => "`$`",
        TokenType::Dot => "`.`",
        TokenType::Shell => "`shell`",
        TokenType::StringKw => "`string`",
        TokenType::Sanctuary => "`sanctuary`",
        TokenType::Import => "`import`",
        TokenType::Var => "`var`",
        TokenType::Pr => "`pr`",
        TokenType::Fn => "`fn`",
        TokenType::Seq => "`seq`",
        TokenType::Par => "`par`",
        TokenType::Env => "`env`",
        TokenType::Case => "`case`",
        TokenType::Log => "`log`",
        TokenType::Exec => "`exec`",
        TokenType::Cd => "`cd`",
        TokenType::Ident(_) => "identifier",
        TokenType::Backtick(_) => "backtick string",
        TokenType::PathLit(_) => "path literal",
        TokenType::Illegal(_) => "illegal token",
        TokenType::EOF => "end of file",
    }
}

fn format_token(token: &Token) -> String {
    match &token.ty {
        TokenType::Ident(s) => format!("`{}`", s),
        TokenType::Backtick(s) => format!("`{}`", s),
        TokenType::PathLit(s) => format!("`{}`", s),
        TokenType::Illegal(s) => format!("`{}`", s),
        _ => format_token_type(&token.ty).to_string(),
    }
}

pub struct Parser {
    lexer: Lexer,
    current: Token,
}

impl Parser {
    pub fn new(mut lexer: Lexer) -> Self {
        let current = lexer.next_token();
        Parser { lexer, current }
    }

    fn current_token(&self) -> &Token {
        &self.current
    }

    fn advance(&mut self) {
        self.current = self.lexer.next_token();
    }

    fn expect_with_context(&mut self, ty: TokenType, context: &str) -> Result<(), ParseError> {
        if std::mem::discriminant(&self.current_token().ty) == std::mem::discriminant(&ty) {
            self.advance();
            Ok(())
        } else {
            let token = self.current_token().clone();
            let expected = format_token_type(&ty);
            let found = format_token(&token);
            Err(ParseError::new(
                SourceSpan::new(token.offset.into(), token.len),
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
        match self.current_token().ty {
            TokenType::Shell => self.parse_shell_decl(),
            TokenType::Sanctuary => self.parse_sanctuary_decl(),
            TokenType::Import => self.parse_import_decl(),
            TokenType::Var => self.parse_var_decl(),
            TokenType::Pr => self.parse_project_decl(),
            _ => Err(ParseError::new(
                miette::SourceSpan::new(
                    self.current_token().offset.into(),
                    self.current_token().len,
                ),
                format!(
                    "expected shell, sanctuary, import, var, or pr, found {}",
                    format_token(self.current_token())
                ),
            )),
        }
    }

    pub(crate) fn parse_project_body_stmt(&mut self) -> Result<Stmt, ParseError> {
        match self.current_token().ty {
            TokenType::Var => self.parse_var_decl(),
            TokenType::Fn => self.parse_fn_decl(),
            TokenType::Seq => self.parse_seq_decl(),
            TokenType::Par => self.parse_par_decl(),
            _ => Err(ParseError::new(
                miette::SourceSpan::new(
                    self.current_token().offset.into(),
                    self.current_token().len,
                ),
                format!(
                    "expected var, fn, seq, or par, found {}",
                    format_token(self.current_token())
                ),
            )),
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
                Shell | Sanctuary | Import | Var | Pr => break,
                _ => self.advance(),
            }
        }
    }

    pub(crate) fn into_source(self) -> String {
        self.lexer.into_source()
    }

    pub(crate) fn parse_fn_stmt(&mut self) -> Result<FnStmt, ParseError> {
        match self.current_token().ty {
            TokenType::Log => self.parse_log_stmt(),
            TokenType::Exec => self.parse_exec_stmt(),
            TokenType::Cd => self.parse_cd_stmt(),
            TokenType::Var => self.parse_fn_var_decl(),
            TokenType::Env => self.parse_env_block(),
            TokenType::Case => self.parse_case_stmt(),
            _ => Err(ParseError::new(
                miette::SourceSpan::new(
                    self.current_token().offset.into(),
                    self.current_token().len,
                ),
                format!(
                    "expected log, exec, cd, var, env, or case, found {}",
                    format_token(self.current_token())
                ),
            )),
        }
    }
}
