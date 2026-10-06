//! Token kinds produced by the lexer.

use crate::syntax::Span;

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum TokenKind {
    Ident(String),
    Text(String),
    Module,
    Import,
    Fn,
    Txt,
    Rec,
    List,
    Mut,
    Switch,
    Case,
    Default,
    Return,
    Panic,
    Async,
    Wait,
    For,
    In,
    Break,
    PathSep,
    Dot,
    Comma,
    Semi,
    Equals,
    Plus,
    Arrow,
    LBrace,
    RBrace,
    LParen,
    RParen,
    LBracket,
    RBracket,
    Eof,
}

#[derive(Debug, PartialEq, Eq)]
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
            TokenKind::List => "`list`".to_owned(),
            TokenKind::Mut => "`mut`".to_owned(),
            TokenKind::Switch => "`switch`".to_owned(),
            TokenKind::Case => "`case`".to_owned(),
            TokenKind::Default => "`default`".to_owned(),
            TokenKind::Return => "`return`".to_owned(),
            TokenKind::Panic => "`panic`".to_owned(),
            TokenKind::Async => "`async`".to_owned(),
            TokenKind::Wait => "`wait`".to_owned(),
            TokenKind::For => "`for`".to_owned(),
            TokenKind::In => "`in`".to_owned(),
            TokenKind::Break => "`break`".to_owned(),
            TokenKind::PathSep => "`::`".to_owned(),
            TokenKind::Dot => "`.`".to_owned(),
            TokenKind::Comma => "`,`".to_owned(),
            TokenKind::Semi => "`;`".to_owned(),
            TokenKind::Equals => "`=`".to_owned(),
            TokenKind::Plus => "`+`".to_owned(),
            TokenKind::Arrow => "`->`".to_owned(),
            TokenKind::LBrace => "`{`".to_owned(),
            TokenKind::RBrace => "`}`".to_owned(),
            TokenKind::LParen => "`(`".to_owned(),
            TokenKind::RParen => "`)`".to_owned(),
            TokenKind::LBracket => "`[`".to_owned(),
            TokenKind::RBracket => "`]`".to_owned(),
            TokenKind::Eof => "the end of the file".to_owned(),
        }
    }
}
