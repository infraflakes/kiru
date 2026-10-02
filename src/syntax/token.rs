//! Token kinds produced by the lexer.

use crate::syntax::Span;

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum TokenKind {
    Ident(String),
    Text(String),
    Module,
    Import,
    Fn,
    Txt,
    Rec,
    Switch,
    Case,
    Default,
    Defer,
    Return,
    Panic,
    PathSep,
    Dot,
    Comma,
    Semi,
    Equals,
    Plus,
    LBrace,
    RBrace,
    LParen,
    RParen,
    Eof,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct Token {
    pub(crate) kind: TokenKind,
    pub(crate) span: Span,
}

impl TokenKind {
    /// A human readable name used in parser diagnostics.
    pub(crate) fn describe(&self) -> String {
        match self {
            TokenKind::Ident(name) => format!("`{name}`"),
            TokenKind::Text(_) => "a string".to_owned(),
            TokenKind::Module => "`module`".to_owned(),
            TokenKind::Import => "`import`".to_owned(),
            TokenKind::Fn => "`fn`".to_owned(),
            TokenKind::Txt => "`txt`".to_owned(),
            TokenKind::Rec => "`rec`".to_owned(),
            TokenKind::Switch => "`switch`".to_owned(),
            TokenKind::Case => "`case`".to_owned(),
            TokenKind::Default => "`default`".to_owned(),
            TokenKind::Defer => "`defer`".to_owned(),
            TokenKind::Return => "`return`".to_owned(),
            TokenKind::Panic => "`panic`".to_owned(),
            TokenKind::PathSep => "`::`".to_owned(),
            TokenKind::Dot => "`.`".to_owned(),
            TokenKind::Comma => "`,`".to_owned(),
            TokenKind::Semi => "`;`".to_owned(),
            TokenKind::Equals => "`=`".to_owned(),
            TokenKind::Plus => "`+`".to_owned(),
            TokenKind::LBrace => "`{`".to_owned(),
            TokenKind::RBrace => "`}`".to_owned(),
            TokenKind::LParen => "`(`".to_owned(),
            TokenKind::RParen => "`)`".to_owned(),
            TokenKind::Eof => "the end of the file".to_owned(),
        }
    }
}
