//! A recursive descent parser for the kiru grammar.
//!
//! This module owns the parser state, the cursor primitives, and the shared
//! Vec machinery. The grammar layers live in the `declarations`,
//! `statements`, and `expressions` submodules.

use crate::syntax::Span;

use super::super::ast::{Expression, Field, File, Item};
use super::super::lex::{Token, TokenKind, WILDCARD, lex};

/// A parse error, positioned at the offending source range.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct ParseError {
    pub(crate) span: Span,
    pub(crate) message: String,
}

/// One parsed source file: the items that parsed and every syntax error the
/// recovery found among the items it dropped.
#[derive(Debug)]
pub(crate) struct Parsed {
    pub(crate) file: File,
    pub(crate) errors: Vec<ParseError>,
}

/// Parse one source file. A lexer error is the one failure that stops parsing:
/// an unterminated string or comment leaves no token boundary to recover to.
/// A syntax error is recorded and the parser resumes at the next item.
pub(crate) fn parse_file(source: &str) -> Result<Parsed, ParseError> {
    let tokens = lex(source).map_err(|error| ParseError {
        span: error.span,
        message: error.message,
    })?;
    Ok(Parser::new(source, tokens).parse_file())
}

/// Cursor state over one lexed token stream.
pub(super) struct Parser<'a> {
    /// The source the tokens were lexed from. An identifier token carries no
    /// word of its own: its span is its word, read from here.
    pub(super) source: &'a str,
    pub(super) tokens: Vec<Token>,
    pub(super) position: usize,
    pub(super) depth: usize,
    /// Every syntax error found while parsing, in the order recovery met them.
    pub(super) errors: Vec<ParseError>,
}

impl<'a> Parser<'a> {
    /// The deepest nesting the parser accepts. A deeper block or map would
    /// exhaust the stack during the recursive descent.
    const MAX_NESTING: usize = 128;

    /// Build a parser over a token stream that ends with `Eof`.
    pub(super) fn new(source: &'a str, tokens: Vec<Token>) -> Self {
        Self {
            source,
            tokens,
            position: 0,
            depth: 0,
            errors: Vec::new(),
        }
    }

    /// Count one level of nesting, rejecting input nested too deeply.
    pub(super) fn enter_nesting(&mut self) -> Result<(), ParseError> {
        self.depth += 1;
        if self.depth > Self::MAX_NESTING {
            return Err(self.error_here("nesting is too deep"));
        }
        Ok(())
    }

    /// Leave one level of nesting.
    pub(super) fn leave_nesting(&mut self) {
        self.depth -= 1;
    }

    /// The token at the cursor, falling back to the final `Eof` token.
    pub(super) fn current(&self) -> &Token {
        self.tokens
            .get(self.position)
            .unwrap_or_else(|| self.tokens.last().expect("the token stream ends with Eof"))
    }

    /// The token `ahead` positions past the cursor, falling back to the final
    /// `Eof` token.
    pub(super) fn peek(&self, ahead: usize) -> &Token {
        self.tokens
            .get(self.position + ahead)
            .unwrap_or_else(|| self.tokens.last().expect("the token stream ends with Eof"))
    }

    /// Whether the cursor token has the same kind discriminant as `kind`.
    pub(super) fn check(&self, kind: &TokenKind) -> bool {
        std::mem::discriminant(&self.current().kind) == std::mem::discriminant(kind)
    }

    /// The word an identifier token spells, out of the source: a token carries
    /// no owned copy, so the span is the word.
    pub(super) fn word(&self, span: Span) -> &'a str {
        &self.source[span.start..span.end]
    }

    /// The cursor token's diagnostic text. A keyword is its one word; an
    /// identifier is the word it spells in the source.
    pub(super) fn describe_current(&self) -> String {
        match &self.current().kind {
            TokenKind::Ident => format!("`{}`", self.word(self.current().span)),
            other => other.describe(),
        }
    }

    /// Consume the token at the cursor and return its span. The token stays in
    /// the buffer, which the cursor never revisits, so consuming moves the
    /// cursor alone and no token is copied.
    pub(super) fn advance(&mut self) -> Span {
        let span = self.current().span;
        if self.position + 1 < self.tokens.len() {
            self.position += 1;
        }
        span
    }

    /// Consume the cursor token when its kind matches, reporting whether it did.
    pub(super) fn eat(&mut self, kind: &TokenKind) -> bool {
        if self.check(kind) {
            self.advance();
            true
        } else {
            false
        }
    }

    /// Consume a token of the expected kind, returning its span, or report the
    /// context in an error.
    pub(super) fn expect(&mut self, kind: &TokenKind, context: &str) -> Result<Span, ParseError> {
        if self.check(kind) {
            Ok(self.advance())
        } else {
            Err(self.error_here(format!(
                "expected {} {context}, found {}",
                kind.describe(),
                self.describe_current()
            )))
        }
    }

    /// Consume a name token, returning the word it spells in the source and
    /// its span.
    pub(super) fn expect_identifier(
        &mut self,
        context: &str,
    ) -> Result<(&'a str, Span), ParseError> {
        match &self.current().kind {
            TokenKind::Ident => {
                let word = self.word(self.current().span);
                let span = self.advance();
                Ok((word, span))
            }
            // A position a name may take accepts the wildcard as its own
            // spelling: `let _ = …;` names a binding the wildcard's one word.
            TokenKind::Wildcard => {
                let span = self.advance();
                Ok((WILDCARD, span))
            }
            other => Err(self.error_here(format!(
                "expected a name {context}, found {}",
                other.describe()
            ))),
        }
    }

    /// Build an error positioned at the cursor token.
    pub(super) fn error_here(&self, message: impl Into<String>) -> ParseError {
        ParseError {
            span: self.current().span,
            message: message.into(),
        }
    }

    /// Parse the token stream into a file: its top-level items in order. A
    /// `use` may stand anywhere among them: it shortens paths for the whole
    /// file's namespace, and the written order between `use`s decides what a
    /// delivered name is on a collision. An item that fails is dropped and
    /// parsing resumes at the next one, so one malformed item never hides the
    /// items after it.
    pub(super) fn parse_file(mut self) -> Parsed {
        let mut items = Vec::new();
        while !self.check(&TokenKind::Eof) {
            let item_start = self.position;
            match self.parse_item() {
                Ok(item) => items.push(item),
                Err(error) => {
                    self.errors.push(error);
                    self.synchronize_item(item_start);
                }
            }
        }
        Parsed {
            file: File { items },
            errors: self.errors,
        }
    }

    /// Whether the cursor token can begin a top-level item.
    pub(super) fn starts_item(&self) -> bool {
        matches!(
            self.current().kind,
            TokenKind::Use
                | TokenKind::Pub
                | TokenKind::Fn
                | TokenKind::Rec
                | TokenKind::Enum
                | TokenKind::Impl
                | TokenKind::Let
        )
    }

    /// Skip the remains of a malformed item that began at `item_start`. The
    /// delimiters the item opened are counted, so a `fn` inside its body is
    /// not mistaken for the next item: recovery stops only at an item keyword
    /// once the item's own nesting has closed, or at the end of the stream.
    /// The token that begins the next item is never consumed.
    pub(super) fn synchronize_item(&mut self, item_start: usize) {
        // An item that failed on its first token consumed nothing, and that
        // token cannot begin a valid item; consuming it keeps recovery from
        // stopping where it started.
        if self.position == item_start {
            self.advance();
        }
        let mut nesting = self.item_nesting(item_start);
        loop {
            if self.check(&TokenKind::Eof) {
                return;
            }
            if nesting <= 0 && self.starts_item() {
                return;
            }
            match self.current().kind {
                TokenKind::LBrace | TokenKind::LParen => nesting += 1,
                TokenKind::RBrace | TokenKind::RParen => nesting -= 1,
                _ => {}
            }
            self.advance();
        }
    }

    /// The delimiter nesting of the tokens an item consumed, counted from the
    /// item's first token.
    pub(super) fn item_nesting(&self, item_start: usize) -> isize {
        self.tokens[item_start..self.position]
            .iter()
            .map(|token| match token.kind {
                TokenKind::LBrace | TokenKind::LParen => 1,
                TokenKind::RBrace | TokenKind::RParen => -1,
                _ => 0,
            })
            .sum()
    }

    /// Parse one top-level item: a `use` or a declaration.
    pub(super) fn parse_item(&mut self) -> Result<Item, ParseError> {
        match &self.current().kind {
            TokenKind::Use => Ok(Item::Use(self.parse_use()?)),
            _ => Ok(Item::Declaration(Box::new(self.parse_declaration()?))),
        }
    }

    /// Consume the semicolon ending a construct, returning the span from
    /// `start` through it.
    pub(super) fn span_through_semicolon(
        &mut self,
        start: usize,
        context: &str,
    ) -> Result<Span, ParseError> {
        let end = self.expect(&TokenKind::Semi, context)?.end;
        Ok(Span::new(start, end))
    }

    /// Parse a qualified `::` path, returning its segments and covering span.
    /// A path begins with a name; a leading `::` is not a form.
    pub(super) fn parse_path_segments(
        &mut self,
        context: &str,
    ) -> Result<(Vec<String>, Span), ParseError> {
        let (first, first_span) = self.expect_identifier(context)?;
        let mut segments = vec![first.to_owned()];
        let mut span = first_span;
        while self.eat(&TokenKind::PathSep) {
            let (segment, segment_span) = self.expect_identifier(context)?;
            segments.push(segment.to_owned());
            span = span.merge(segment_span);
        }
        Ok((segments, span))
    }

    /// Parse a `{ ... }` record literal, returning its fields and covering span.
    pub(super) fn parse_rec_fields(&mut self) -> Result<(Vec<Field>, Span), ParseError> {
        self.parse_delimited(
            &TokenKind::LBrace,
            &TokenKind::RBrace,
            "to open a map",
            "to close a map",
            Self::parse_rec_field,
        )
    }

    /// Parse one `name = expression` map field.
    pub(super) fn parse_rec_field(&mut self) -> Result<Field, ParseError> {
        let (name, name_span) = self.expect_identifier("in a map field")?;
        self.expect(&TokenKind::Equals, "after the field name")?;
        let value = self.parse_expression()?;
        let span = name_span.merge(value.span());
        Ok(Field {
            name: name.to_owned(),
            name_span,
            value,
            span,
        })
    }

    /// Parse an argument list, returning the arguments and covering span.
    pub(super) fn parse_arguments(&mut self) -> Result<(Vec<Expression>, Span), ParseError> {
        self.parse_delimited(
            &TokenKind::LParen,
            &TokenKind::RParen,
            "to open the argument list",
            "to close the argument list",
            Self::parse_expression,
        )
    }

    /// Parse a comma separated Vec between `open` and `close`, returning the
    /// items and the span from the opening to the closing token.
    pub(super) fn parse_delimited<T>(
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
