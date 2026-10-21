//! Lexing: source text becomes tokens.
//!
//! `scanner` walks the characters in place; `token` declares the token kinds a
//! scanner emits and the words the language reserves.

mod scanner;
mod token;

pub(crate) use scanner::{lex, lex_with_comments};
pub(crate) use token::{Token, TokenKind, WILDCARD};
