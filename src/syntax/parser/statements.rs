//! Statement level grammar: blocks, statements, switches, and the statement
//! forms that close with a semicolon.

use crate::syntax::ast::{Case, Statement, ValueKind};
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
            TokenKind::Txt => Ok(Statement::Binding(self.parse_binding(ValueKind::Text)?)),
            TokenKind::Rec => Ok(Statement::Binding(self.parse_binding(ValueKind::Record)?)),
            TokenKind::Return => self.parse_return(),
            TokenKind::Panic => self.parse_panic(),
            TokenKind::Async => self.parse_async(),
            TokenKind::Wait => self.parse_wait(),
            TokenKind::Defer => self.parse_defer(),
            TokenKind::Switch => self.parse_switch(),
            TokenKind::Ident(_) if matches!(self.peek(1).kind, TokenKind::Equals) => {
                self.parse_assignment()
            }
            _ => {
                let expression = self.parse_expression()?;
                self.expect(&TokenKind::Semi, "after the expression")?;
                Ok(Statement::Expression(expression))
            }
        }
    }

    /// Parse a `return();` or `return(expression);` statement.
    fn parse_return(&mut self) -> Result<Statement, ParseError> {
        let start = self.advance().start;
        self.expect(&TokenKind::LParen, "after `return`")?;
        let value = if self.check(&TokenKind::RParen) {
            None
        } else {
            Some(self.parse_expression()?)
        };
        self.expect(&TokenKind::RParen, "after the returned value")?;
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

    /// Parse a `defer { ... };` statement.
    fn parse_defer(&mut self) -> Result<Statement, ParseError> {
        let start = self.advance().start;
        let body = self.parse_block()?;
        let span = self.span_through_semicolon(start, "after the defer block")?;
        Ok(Statement::Defer { body, span })
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

    /// Parse a `switch (subject) { ... };` statement.
    fn parse_switch(&mut self) -> Result<Statement, ParseError> {
        let start = self.advance().start;
        self.expect(&TokenKind::LParen, "after `switch`")?;
        let subject = self.parse_expression()?;
        self.expect(&TokenKind::RParen, "after the switch subject")?;
        self.expect(&TokenKind::LBrace, "to open the switch")?;

        let mut cases = Vec::new();
        let mut default = None;
        while !self.check(&TokenKind::RBrace) {
            if self.check(&TokenKind::Case) {
                let case_start = self.advance().start;
                self.expect(&TokenKind::LParen, "after `case`")?;
                let pattern = self.parse_expression()?;
                self.expect(&TokenKind::RParen, "after the case pattern")?;
                let body = self.parse_block()?;
                let span = self.span_through_semicolon(case_start, "after the case body")?;
                cases.push(Case {
                    pattern,
                    body,
                    span,
                });
            } else if self.check(&TokenKind::Default) {
                if default.is_some() {
                    return Err(self.error_here("a switch allows at most one `default`"));
                }
                self.advance();
                let body = self.parse_block()?;
                self.expect(&TokenKind::Semi, "after the default body")?;
                default = Some(body);
            } else {
                return Err(self.error_here(format!(
                    "expected `case` or `default`, found {}",
                    self.current().kind.describe()
                )));
            }
        }
        self.expect(&TokenKind::RBrace, "to close the switch")?;
        let span = self.span_through_semicolon(start, "after the switch")?;
        Ok(Statement::Switch {
            subject,
            cases,
            default,
            span,
        })
    }
}
