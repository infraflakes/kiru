//! A recursive descent parser for the kiru grammar.
//!
//! This module owns the parser state, the cursor primitives, and the shared
//! list machinery. The grammar layers live in the `declarations`,
//! `statements`, and `expressions` submodules.

mod declarations;
mod expressions;
mod statements;

#[cfg(test)]
mod tests;

use crate::syntax::Span;

use super::ast::{Expression, Field, File, Item};
use super::lexer::lex;
use super::token::{Token, TokenKind};

/// A parse error, positioned at the offending source range.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct ParseError {
    pub(crate) span: Span,
    pub(crate) message: String,
}

/// Parse one source file.
pub(crate) fn parse_file(source: &str) -> Result<File, ParseError> {
    let tokens = lex(source).map_err(|error| ParseError {
        span: error.span,
        message: error.message,
    })?;
    Parser::new(tokens).parse_file()
}

/// Cursor state over one lexed token stream.
struct Parser {
    tokens: Vec<Token>,
    position: usize,
    depth: usize,
}

impl Parser {
    /// The deepest nesting the parser accepts. A deeper block or record would
    /// exhaust the stack during the recursive descent.
    const MAX_NESTING: usize = 128;

    /// Build a parser over a token stream that ends with `Eof`.
    fn new(tokens: Vec<Token>) -> Self {
        Self {
            tokens,
            position: 0,
            depth: 0,
        }
    }

    /// Count one level of nesting, rejecting input nested too deeply.
    fn enter_nesting(&mut self) -> Result<(), ParseError> {
        self.depth += 1;
        if self.depth > Self::MAX_NESTING {
            return Err(self.error_here("nesting is too deep"));
        }
        Ok(())
    }

    /// Leave one level of nesting.
    fn leave_nesting(&mut self) {
        self.depth -= 1;
    }

    /// The token at the cursor, falling back to the final `Eof` token.
    fn current(&self) -> &Token {
        self.tokens
            .get(self.position)
            .unwrap_or_else(|| self.tokens.last().expect("the token stream ends with Eof"))
    }

    /// The token `ahead` positions past the cursor, falling back to the final
    /// `Eof` token.
    fn peek(&self, ahead: usize) -> &Token {
        self.tokens
            .get(self.position + ahead)
            .unwrap_or_else(|| self.tokens.last().expect("the token stream ends with Eof"))
    }

    /// Whether the cursor token has the same kind discriminant as `kind`.
    fn check(&self, kind: &TokenKind) -> bool {
        std::mem::discriminant(&self.current().kind) == std::mem::discriminant(kind)
    }

    /// Consume the token at the cursor and return its span. The token stays in
    /// the buffer, which the cursor never revisits, so consuming moves the
    /// cursor alone and no token is copied.
    fn advance(&mut self) -> Span {
        let span = self.current().span;
        if self.position + 1 < self.tokens.len() {
            self.position += 1;
        }
        span
    }

    /// Consume the cursor token when its kind matches, reporting whether it did.
    fn eat(&mut self, kind: &TokenKind) -> bool {
        if self.check(kind) {
            self.advance();
            true
        } else {
            false
        }
    }

    /// Consume a token of the expected kind, returning its span, or report the
    /// context in an error.
    fn expect(&mut self, kind: &TokenKind, context: &str) -> Result<Span, ParseError> {
        if self.check(kind) {
            Ok(self.advance())
        } else {
            Err(self.error_here(format!(
                "expected {} {context}, found {}",
                kind.describe(),
                self.current().kind.describe()
            )))
        }
    }

    /// Consume a name token, returning its text and span.
    fn expect_identifier(&mut self, context: &str) -> Result<(String, Span), ParseError> {
        match &self.current().kind {
            TokenKind::Ident(name) => {
                let name = name.clone();
                let span = self.advance();
                Ok((name, span))
            }
            other => Err(self.error_here(format!(
                "expected a name {context}, found {}",
                other.describe()
            ))),
        }
    }

    /// Consume a string token, returning its text and span.
    fn expect_text(&mut self, context: &str) -> Result<(String, Span), ParseError> {
        match &self.current().kind {
            TokenKind::Text(value) => {
                let value = value.clone();
                let span = self.advance();
                Ok((value, span))
            }
            other => Err(self.error_here(format!(
                "expected a string {context}, found {}",
                other.describe()
            ))),
        }
    }

    /// Build an error positioned at the cursor token.
    fn error_here(&self, message: impl Into<String>) -> ParseError {
        ParseError {
            span: self.current().span,
            message: message.into(),
        }
    }

    /// Parse the token stream into a file: its top-level items in order.
    fn parse_file(mut self) -> Result<File, ParseError> {
        let mut items = Vec::new();
        while !self.check(&TokenKind::Eof) {
            items.push(self.parse_item()?);
        }
        Ok(File { items })
    }

    /// Parse one top-level item: an `import`, a `mod` block, or a declaration.
    fn parse_item(&mut self) -> Result<Item, ParseError> {
        match &self.current().kind {
            TokenKind::Import => Ok(Item::Import(self.parse_import()?)),
            TokenKind::Mod => Ok(Item::Module(self.parse_mod()?)),
            _ => Ok(Item::Declaration(self.parse_declaration()?)),
        }
    }

    /// Consume the semicolon ending a construct, returning the span from
    /// `start` through it.
    fn span_through_semicolon(&mut self, start: usize, context: &str) -> Result<Span, ParseError> {
        let end = self.expect(&TokenKind::Semi, context)?.end;
        Ok(Span::new(start, end))
    }

    /// Parse a qualified `::` path, returning its segments and covering span.
    fn parse_path_segments(&mut self, context: &str) -> Result<(Vec<String>, Span), ParseError> {
        let (first, first_span) = self.expect_identifier(context)?;
        let mut segments = vec![first];
        let mut span = first_span;
        while self.eat(&TokenKind::PathSep) {
            let (segment, segment_span) = self.expect_identifier(context)?;
            segments.push(segment);
            span = span.merge(segment_span);
        }
        Ok((segments, span))
    }

    /// Parse a qualified name that may open with `::`, returning whether the
    /// path is rooted explicitly, its segments, and its covering span.
    fn parse_rooted_path(
        &mut self,
        context: &str,
    ) -> Result<(bool, Vec<String>, Span), ParseError> {
        let root_span = if self.check(&TokenKind::PathSep) {
            Some(self.advance())
        } else {
            None
        };
        let (segments, span) = self.parse_path_segments(context)?;
        let span = root_span.map_or(span, |root| root.merge(span));
        Ok((root_span.is_some(), segments, span))
    }

    /// Parse a `{ ... }` record literal, returning its fields and covering span.
    fn parse_record_fields(&mut self) -> Result<(Vec<Field>, Span), ParseError> {
        self.parse_delimited(
            &TokenKind::LBrace,
            &TokenKind::RBrace,
            "to open a record",
            "to close a record",
            Self::parse_record_field,
        )
    }

    /// Parse one `name = expression` record field.
    fn parse_record_field(&mut self) -> Result<Field, ParseError> {
        let (name, name_span) = self.expect_identifier("in a record field")?;
        self.expect(&TokenKind::Equals, "after the field name")?;
        let value = self.parse_expression()?;
        let span = name_span.merge(value.span());
        Ok(Field {
            name,
            name_span,
            value,
            span,
        })
    }

    /// Parse a `[ ... ]` list literal, returning its elements and covering
    /// span.
    fn parse_list_elements(&mut self) -> Result<(Vec<Expression>, Span), ParseError> {
        self.parse_delimited(
            &TokenKind::LBracket,
            &TokenKind::RBracket,
            "to open a list",
            "to close a list",
            Self::parse_expression,
        )
    }

    /// Parse an argument list, returning the arguments and covering span.
    fn parse_arguments(&mut self) -> Result<(Vec<Expression>, Span), ParseError> {
        self.parse_delimited(
            &TokenKind::LParen,
            &TokenKind::RParen,
            "to open the argument list",
            "to close the argument list",
            Self::parse_expression,
        )
    }

    /// Parse a comma separated list between `open` and `close`, returning the
    /// items and the span from the opening to the closing token.
    fn parse_delimited<T>(
        &mut self,
        open: &TokenKind,
        close: &TokenKind,
        open_context: &str,
        close_context: &str,
        mut parse_item: impl FnMut(&mut Self) -> Result<T, ParseError>,
    ) -> Result<(Vec<T>, Span), ParseError> {
        let open_span = self.expect(open, open_context)?;
        let mut items = Vec::new();
        if !self.check(close) {
            loop {
                items.push(parse_item(self)?);
                if !self.eat(&TokenKind::Comma) {
                    break;
                }
                if self.check(close) {
                    break;
                }
            }
        }
        let close_span = self.expect(close, close_context)?;
        Ok((items, open_span.merge(close_span)))
    }
}
