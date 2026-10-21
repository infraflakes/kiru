//! Lexing and parsing for kiru source: tokens, the syntax tree, and spans.

mod ast;
mod lex;
mod parse;
mod span;

pub(crate) use ast::{
    Arithmetic, BinaryInstruction, BinaryOp, BinaryOperands, Comparison, Declaration, Expression,
    Field, Function, Item, MatchBody, Parameter, Statement, StringPart, TypeDeclaration, TypeExpr,
    UnaryOp,
};
pub(crate) use lex::{Token, TokenKind, WILDCARD, lex_with_comments};
pub(crate) use parse::parse_file;
pub use span::Span;
