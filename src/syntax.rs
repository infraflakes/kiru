//! Syntax layer: lexer, parser, and the unresolved AST. The engine
//! (`crate::compile`, `crate::exec`) consumes what is parsed here, never
//! the reverse.
//!
//! Where new syntax plugs in - the seams, one place each:
//!
//! - a keyword: one `TokenType` variant, one `KEYWORDS` row (its
//!   `KeywordForm` decides fusion, reservedness, and display), one arm in
//!   `fuse_call_arguments` if it is call-form, one parser dispatch arm;
//! - a statement: one `FnStmt` variant (or top-level `Stmt` variant), one
//!   parser dispatch arm, one lowering arm in `crate::compile::inline`,
//!   one runtime arm in `crate::exec::context`, IR serialize/deserialize;
//! - a template segment: one `Part` variant, inlining in
//!   `crate::compile::inline::inline_dsl_parts`.
//!
//! Rust's exhaustive matches turn each addition into a compiler-guided
//! checklist; the resolver and lowering seams are the only semantic
//! decision points.

pub(crate) mod ast;
pub(crate) mod error;
pub(crate) mod fnstmt;
pub(crate) mod lexer;
pub(crate) mod parser;
pub(crate) mod source;
pub(crate) mod token;

pub(crate) use ast::{Program, Stmt, TopLevel};
pub(crate) use fnstmt::FnStmt;
pub(crate) use parser::Parser;
pub(crate) use source::{Part, Template};
