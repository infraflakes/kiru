//! Statement level grammar: blocks, statements, matches, and the statement
//! forms that close with a semicolon.

use crate::syntax::Span;
use crate::syntax::ast::{Expression, MatchArm, MatchBody, Statement};
use crate::syntax::token::TokenKind;

use super::{ParseError, Parser};

impl<'a> Parser<'a> {
    /// Parse a `{ ... }` block into its statements. A statement that fails is
    /// dropped and its error recorded, and the statements after it are still
    /// read: one malformed statement does not hide the rest of the body.
    pub(super) fn parse_block(&mut self) -> Result<Vec<Statement>, ParseError> {
        self.enter_nesting()?;
        self.expect(&TokenKind::LBrace, "to open a block")?;
        let mut statements = Vec::new();
        while !self.check(&TokenKind::RBrace) {
            if self.check(&TokenKind::Eof) {
                return Err(
                    self.error_here("expected `}` to close the block, found the end of the file")
                );
            }
            match self.parse_statement() {
                Ok(statement) => statements.push(statement),
                Err(error) => {
                    self.errors.push(error);
                    self.synchronize_statement();
                }
            }
        }
        self.expect(&TokenKind::RBrace, "to close the block")?;
        self.leave_nesting();
        Ok(statements)
    }

    /// Skip the remains of a malformed statement: stop after the `;` that
    /// closes it at the body's own nesting, or before the `}` that closes the
    /// body, which the block parser consumes. A `;` inside a nested block,
    /// argument list, or map belongs to that construct, not to this body.
    fn synchronize_statement(&mut self) {
        let mut nesting = 0usize;
        loop {
            if self.check(&TokenKind::Eof) {
                return;
            }
            if nesting == 0 && self.check(&TokenKind::RBrace) {
                return;
            }
            if nesting == 0 && self.check(&TokenKind::Semi) {
                self.advance();
                return;
            }
            match self.current().kind {
                TokenKind::LBrace | TokenKind::LParen => nesting += 1,
                TokenKind::RBrace | TokenKind::RParen => nesting = nesting.saturating_sub(1),
                _ => {}
            }
            self.advance();
        }
    }

    /// Dispatch on the cursor token to parse one statement.
    fn parse_statement(&mut self) -> Result<Statement, ParseError> {
        match &self.current().kind {
            TokenKind::Let => Ok(Statement::Binding(self.parse_let_binding()?)),
            TokenKind::Return => self.parse_return(),
            TokenKind::Panic => self.parse_panic(),
            TokenKind::Async => self.parse_async(),
            TokenKind::Wait => self.parse_wait(),
            TokenKind::Match => {
                let expression = self.parse_match(false)?;
                self.expect(&TokenKind::Semi, "after the match")?;
                Ok(Statement::Expression(expression))
            }
            TokenKind::For => self.parse_for(),
            TokenKind::Break => self.parse_break(),
            TokenKind::Ident | TokenKind::Wildcard
                if matches!(self.peek(1).kind, TokenKind::Equals) =>
            {
                self.parse_assignment()
            }
            TokenKind::Ident | TokenKind::Wildcard
                if matches!(self.peek(1).kind, TokenKind::Dot)
                    && matches!(self.peek(2).kind, TokenKind::Ident | TokenKind::Wildcard)
                    && matches!(self.peek(3).kind, TokenKind::Equals) =>
            {
                self.parse_field_assignment()
            }
            _ => {
                let expression = self.parse_expression()?;
                self.expect(&TokenKind::Semi, "after the expression")?;
                Ok(Statement::Expression(expression))
            }
        }
    }

    /// Parse a `return;` or `return expression;` statement.
    fn parse_return(&mut self) -> Result<Statement, ParseError> {
        let start = self.advance().start;
        let value = if self.check(&TokenKind::Semi) {
            None
        } else {
            Some(self.parse_expression()?)
        };
        let span = self.span_through_semicolon(start, "after the return")?;
        Ok(Statement::Return { value, span })
    }

    /// Parse a `panic;` statement.
    fn parse_panic(&mut self) -> Result<Statement, ParseError> {
        let start = self.advance().start;
        let span = self.span_through_semicolon(start, "after `panic`")?;
        Ok(Statement::Panic { span })
    }

    /// Parse an `async <call>;` statement. The call is any expression; the
    /// checker requires it to be a call.
    fn parse_async(&mut self) -> Result<Statement, ParseError> {
        let start = self.advance().start;
        let call = self.parse_expression()?;
        let span = self.span_through_semicolon(start, "after `async`")?;
        Ok(Statement::Async { call, span })
    }

    /// Parse a `wait;` statement.
    fn parse_wait(&mut self) -> Result<Statement, ParseError> {
        let start = self.advance().start;
        let span = self.span_through_semicolon(start, "after `wait`")?;
        Ok(Statement::Wait { span })
    }

    /// Parse a `for { ... };` statement, which repeats until a `break`.
    fn parse_for(&mut self) -> Result<Statement, ParseError> {
        let start = self.advance().start;
        let body = self.parse_block()?;
        let span = self.span_through_semicolon(start, "after the for body")?;
        Ok(Statement::Forever { body, span })
    }

    /// Parse a `break;` statement.
    fn parse_break(&mut self) -> Result<Statement, ParseError> {
        let start = self.advance().start;
        let span = self.span_through_semicolon(start, "after `break`")?;
        Ok(Statement::Break { span })
    }

    /// Parse a `name = expression;` assignment statement.
    fn parse_assignment(&mut self) -> Result<Statement, ParseError> {
        let (name, name_span) = self.expect_identifier("in an assignment")?;
        let start = name_span.start;
        self.expect(&TokenKind::Equals, "after the assigned name")?;
        let value = self.parse_expression()?;
        let span = self.span_through_semicolon(start, "after the assignment")?;
        Ok(Statement::Assignment {
            name: name.to_owned(),
            name_span,
            value,
            span,
        })
    }

    /// Parse a `name.field = expression;` field assignment statement.
    fn parse_field_assignment(&mut self) -> Result<Statement, ParseError> {
        let (name, name_span) = self.expect_identifier("in a field assignment")?;
        let start = name_span.start;
        self.expect(&TokenKind::Dot, "after the assigned name")?;
        let (field, field_span) = self.expect_identifier("in a field assignment")?;
        self.expect(&TokenKind::Equals, "after the assigned field")?;
        let value = self.parse_expression()?;
        let span = self.span_through_semicolon(start, "after the field assignment")?;
        Ok(Statement::FieldAssignment {
            name: name.to_owned(),
            name_span,
            field: field.to_owned(),
            field_span,
            value,
            span,
        })
    }

    /// Parse a `match subject { pattern => body; … };` term. In statement
    /// position (`value` is false) the arm bodies are blocks; in expression
    /// position they are expressions. The `_` arm is the default.
    pub(super) fn parse_match(&mut self, value: bool) -> Result<Expression, ParseError> {
        let start = self.advance().start;
        // The subject may end in a bare `{ ... }` shape, so its `{` opens the
        // subject and the body's `{` follows it.
        let subject = self.parse_expression()?;
        self.expect(&TokenKind::LBrace, "to open the match")?;

        let mut cases = Vec::new();
        let mut default = None;
        while !self.check(&TokenKind::RBrace) {
            if self.check(&TokenKind::Eof) {
                return Err(
                    self.error_here("expected `}` to close the match, found the end of the file")
                );
            }
            if default.is_some() {
                return Err(self.error_here("the `_` arm must be last"));
            }
            let arm_start = self.current().span.start;
            let is_default = self.check_wildcard_arm();
            let pattern = if is_default {
                self.advance();
                None
            } else {
                Some(self.parse_expression()?)
            };
            self.expect(&TokenKind::FatArrow, "after the pattern")?;
            let body = if value {
                MatchBody::Expression(Box::new(self.parse_expression()?))
            } else {
                MatchBody::Block(self.parse_block()?)
            };
            let span = self.span_through_semicolon(arm_start, "after the match arm")?;
            match pattern {
                Some(pattern) => cases.push(MatchArm {
                    pattern,
                    body,
                    span,
                }),
                None => default = Some(body),
            }
        }
        let close = self.expect(&TokenKind::RBrace, "to close the match")?;
        let span = Span::new(start, close.end);
        Ok(Expression::Match {
            subject: Box::new(subject),
            cases,
            default,
            span,
        })
    }

    /// Whether the cursor opens the `_ =>` default arm of a match.
    fn check_wildcard_arm(&self) -> bool {
        matches!(&self.current().kind, TokenKind::Wildcard)
            && matches!(self.peek(1).kind, TokenKind::FatArrow)
    }
}
