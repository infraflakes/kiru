//! Token kinds produced by the lexer.

use crate::syntax::Span;

/// The wildcard's one written word. It is a word of its own kind: a pass reads
/// the kind and never re-derives it from the identifier's spelling.
pub(crate) const WILDCARD: &str = "_";

#[derive(Debug, PartialEq, Eq)]
pub(crate) enum TokenKind {
    /// An identifier: its one word is its span in the source, so the token
    /// owns no copy and the parser reads the spelling out of the source.
    Ident,
    /// The wildcard `_`: a match's default arm, and the payload binding a
    /// variant pattern ignores.
    Wildcard,
    Text(String),
    /// An integer literal.
    Int(i64),
    /// The `@(` that opens an interpolation inside a string.
    InterpolationStart,
    /// The `)` that closes an interpolation inside a string.
    InterpolationEnd,
    /// `use`, which shortens paths: it brings a namespace's qualifier, a type,
    /// or a plain function into the namespace that wrote it.
    Use,
    /// `pub`, which lets a declaration be seen outside its namespace.
    Pub,
    Fn,
    /// `impl`, which opens an `impl` block that files functions under a type.
    Impl,
    /// `enum`, which opens an enum type declaration.
    Enum,
    Let,
    /// `rec`, which opens a nominal `rec` type declaration.
    Rec,
    Match,
    Return,
    Panic,
    Async,
    Wait,
    For,
    Break,
    True,
    False,
    PathSep,
    Dot,
    Colon,
    Comma,
    Semi,
    Equals,
    EqEq,
    NotEq,
    Plus,
    Minus,
    Star,
    Slash,
    Percent,
    Bang,
    AndAnd,
    OrOr,
    Arrow,
    FatArrow,
    Less,
    LessEq,
    Greater,
    GreaterEq,
    LBrace,
    RBrace,
    LParen,
    RParen,
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
            TokenKind::Ident => "a name".to_owned(),
            TokenKind::Wildcard => "`_`".to_owned(),
            TokenKind::Text(_) => "a string".to_owned(),
            TokenKind::Int(value) => format!("`{value}`"),
            TokenKind::InterpolationStart => "`@(`".to_owned(),
            TokenKind::InterpolationEnd => "`)`".to_owned(),
            TokenKind::Use => "`use`".to_owned(),
            TokenKind::Pub => "`pub`".to_owned(),
            TokenKind::Fn => "`fn`".to_owned(),
            TokenKind::Impl => "`impl`".to_owned(),
            TokenKind::Enum => "`enum`".to_owned(),
            TokenKind::Let => "`let`".to_owned(),
            TokenKind::Rec => "`rec`".to_owned(),
            TokenKind::Match => "`match`".to_owned(),
            TokenKind::Return => "`return`".to_owned(),
            TokenKind::Panic => "`panic`".to_owned(),
            TokenKind::Async => "`async`".to_owned(),
            TokenKind::Wait => "`wait`".to_owned(),
            TokenKind::For => "`for`".to_owned(),
            TokenKind::Break => "`break`".to_owned(),
            TokenKind::True => "`true`".to_owned(),
            TokenKind::False => "`false`".to_owned(),
            TokenKind::PathSep => "`::`".to_owned(),
            TokenKind::Dot => "`.`".to_owned(),
            TokenKind::Colon => "`:`".to_owned(),
            TokenKind::Comma => "`,`".to_owned(),
            TokenKind::Semi => "`;`".to_owned(),
            TokenKind::Equals => "`=`".to_owned(),
            TokenKind::EqEq => "`==`".to_owned(),
            TokenKind::NotEq => "`!=`".to_owned(),
            TokenKind::Plus => "`+`".to_owned(),
            TokenKind::Minus => "`-`".to_owned(),
            TokenKind::Star => "`*`".to_owned(),
            TokenKind::Slash => "`/`".to_owned(),
            TokenKind::Percent => "`%`".to_owned(),
            TokenKind::Bang => "`!`".to_owned(),
            TokenKind::AndAnd => "`&&`".to_owned(),
            TokenKind::OrOr => "`||`".to_owned(),
            TokenKind::Arrow => "`->`".to_owned(),
            TokenKind::FatArrow => "`=>`".to_owned(),
            TokenKind::Less => "`<`".to_owned(),
            TokenKind::LessEq => "`<=`".to_owned(),
            TokenKind::Greater => "`>`".to_owned(),
            TokenKind::GreaterEq => "`>=`".to_owned(),
            TokenKind::LBrace => "`{`".to_owned(),
            TokenKind::RBrace => "`}`".to_owned(),
            TokenKind::LParen => "`(`".to_owned(),
            TokenKind::RParen => "`)`".to_owned(),
            TokenKind::Eof => "the end of the file".to_owned(),
        }
    }
}
