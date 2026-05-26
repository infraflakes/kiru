use crate::dsl::token::{Token, TokenType};
use miette::{Diagnostic, SourceSpan};
use thiserror::Error;

#[derive(Debug, Error, Diagnostic)]
#[error("{msg}")]
pub struct ParseError {
    #[label("{msg}")]
    span: SourceSpan,
    msg: String,
}

impl ParseError {
    pub fn new(span: SourceSpan, msg: String) -> Self {
        Self { span, msg }
    }
}

pub fn format_token_type(ty: &TokenType) -> &'static str {
    match ty {
        TokenType::LBrace => "`{`",
        TokenType::RBrace => "`}`",
        TokenType::LBracket => "`[`",
        TokenType::RBracket => "`]`",
        TokenType::Semicolon => "`;`",
        TokenType::Comma => "`,`",
        TokenType::Assign => "`=`",
        TokenType::Dollar => "`$`",
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

pub fn format_token(token: &Token) -> String {
    match &token.ty {
        TokenType::Ident(s) => format!("`{}`", s),
        TokenType::Backtick(s) => format!("`{}`", s),
        TokenType::PathLit(s) => format!("`{}`", s),
        TokenType::Illegal(s) => format!("`{}`", s),
        _ => format_token_type(&token.ty).to_string(),
    }
}

pub fn is_keyword_token(ty: &TokenType) -> bool {
    matches!(
        ty,
        TokenType::Log
            | TokenType::Exec
            | TokenType::Cd
            | TokenType::Case
            | TokenType::Env
            | TokenType::Var
            | TokenType::Fn
            | TokenType::Seq
            | TokenType::Par
            | TokenType::Pr
            | TokenType::Shell
            | TokenType::StringKw
            | TokenType::Sanctuary
            | TokenType::Import
    )
}
