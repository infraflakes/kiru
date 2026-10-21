//! Lexing and parsing for kiru source.

mod ast;
mod lexer;
mod parser;
mod span;
mod token;

pub(crate) use ast::{
    Arithmetic, BinaryInstruction, BinaryOp, BinaryOperands, Comparison, Declaration, Expression,
    Field, Function, Item, MatchBody, Parameter, Statement, StringPart, TypeDeclaration, TypeExpr,
    UnaryOp,
};
pub(crate) use lexer::lex_with_comments;
pub(crate) use parser::parse_file;
pub use span::Span;
pub(crate) use token::WILDCARD;
pub(crate) use token::{Token, TokenKind};
