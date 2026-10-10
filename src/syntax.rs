//! Lexing and parsing for kiru source.

mod ast;
mod lexer;
mod parser;
mod span;
mod token;

pub(crate) use ast::{
    Declaration, Expression, Field, Item, MatchBody, Module, Statement, StringPart,
};
pub(crate) use parser::parse_file;
pub(crate) use span::Span;
