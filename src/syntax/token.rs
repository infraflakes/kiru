//! Token kinds produced by the lexer.

use crate::syntax::Span;
use crate::types::Type;

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum TokenKind {
    Ident(String),
    Text(String),
    /// The `@(` that opens an interpolation inside a string.
    InterpolationStart,
    /// The `)` that closes an interpolation inside a string.
    InterpolationEnd,
    Mod,
    Import,
    Fn,
    Let,
    Type(Type),
    Mut,
    Match,
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
    FatArrow,
    Less,
    Greater,
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
            TokenKind::InterpolationStart => "`@(`".to_owned(),
            TokenKind::InterpolationEnd => "`)`".to_owned(),
            TokenKind::Mod => "`mod`".to_owned(),
            TokenKind::Import => "`import`".to_owned(),
            TokenKind::Fn => "`fn`".to_owned(),
            TokenKind::Let => "`let`".to_owned(),
            TokenKind::Type(ty) => format!("`{}`", ty.keyword().unwrap_or(ty.name())),
            TokenKind::Mut => "`mut`".to_owned(),
            TokenKind::Match => "`match`".to_owned(),
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
            TokenKind::FatArrow => "`=>`".to_owned(),
            TokenKind::Less => "`<`".to_owned(),
            TokenKind::Greater => "`>`".to_owned(),
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
