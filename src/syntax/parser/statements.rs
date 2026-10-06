//! Statement level grammar: blocks, statements, matches, and the statement
//! forms that close with a semicolon.

use crate::syntax::Span;
use crate::syntax::ast::{Expression, MatchArm, MatchBody, Statement};
use crate::syntax::token::TokenKind;

use super::{ParseError, Parser};

impl Parser {
    /// Parse a `{ ... }` block into its statements.
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
            statements.push(self.parse_statement()?);
        }
        self.expect(&TokenKind::RBrace, "to close the block")?;
        self.leave_nesting();
        Ok(statements)
    }

    /// Dispatch on the cursor token to parse one statement.
    fn parse_statement(&mut self) -> Result<Statement, ParseError> {
        match &self.current().kind {
            TokenKind::Let => Ok(Statement::Binding(self.parse_let_binding()?)),
            TokenKind::Mut => Err(self.error_here("`mut` must follow `let`")),
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
            TokenKind::Ident(_) if matches!(self.peek(1).kind, TokenKind::Equals) => {
                self.parse_assignment()
            }
            TokenKind::Ident(_) if matches!(self.peek(1).kind, TokenKind::Dot) => {
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

    /// Parse a `for` statement: `for item in <list> { ... };` iterates a list,
    /// and `for { ... };` repeats until a `break`.
    fn parse_for(&mut self) -> Result<Statement, ParseError> {
        let start = self.advance().start;
        if self.check(&TokenKind::LBrace) {
            let body = self.parse_block()?;
            let span = self.span_through_semicolon(start, "after the for body")?;
            return Ok(Statement::Forever { body, span });
        }
        let (item, item_span) = self.expect_identifier("as the loop variable")?;
        self.expect(&TokenKind::In, "after the loop variable")?;
        let iterable = self.parse_expression()?;
        let body = self.parse_block()?;
        let span = self.span_through_semicolon(start, "after the for body")?;
        Ok(Statement::ForEach {
            item,
            item_span,
            iterable,
            body,
            span,
        })
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
            name,
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
            name,
            name_span,
            field,
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
                None => {
                    if default.is_some() {
                        return Err(self.error_here("a match allows at most one `_` arm"));
                    }
                    default = Some(body);
                }
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
        matches!(&self.current().kind, TokenKind::Ident(name) if name == "_")
            && matches!(self.peek(1).kind, TokenKind::FatArrow)
    }
}
