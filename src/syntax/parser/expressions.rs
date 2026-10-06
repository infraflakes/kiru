//! Expression grammar: sums of postfix expressions and primary expressions.

use crate::syntax::Span;
use crate::syntax::ast::{Expression, StringPart};
use crate::syntax::token::TokenKind;

use super::{ParseError, Parser};

impl Parser {
    /// Parse a sum of postfix expressions.
    pub(super) fn parse_expression(&mut self) -> Result<Expression, ParseError> {
        self.enter_nesting()?;
        let expression = self.parse_sum()?;
        self.leave_nesting();
        Ok(expression)
    }

    /// Parse a sum of postfix expressions.
    fn parse_sum(&mut self) -> Result<Expression, ParseError> {
        let mut left = self.parse_postfix()?;
        while self.check(&TokenKind::Plus) {
            self.advance();
            let right = self.parse_postfix()?;
            let span = left.span().merge(right.span());
            left = Expression::Add {
                left: Box::new(left),
                right: Box::new(right),
                span,
            };
        }
        Ok(left)
    }

    /// Parse a primary expression followed by any field accesses.
    fn parse_postfix(&mut self) -> Result<Expression, ParseError> {
        let mut expression = self.parse_primary()?;
        while self.eat(&TokenKind::Dot) {
            let (name, name_span) = self.expect_identifier("after `.`")?;
            let span = expression.span().merge(name_span);
            expression = Expression::Field {
                target: Box::new(expression),
                name,
                name_span,
                span,
            };
        }
        Ok(expression)
    }

    /// Parse a literal, a record, a name, or a call.
    fn parse_primary(&mut self) -> Result<Expression, ParseError> {
        match &self.current().kind {
            TokenKind::Text(_) | TokenKind::InterpolationStart => self.parse_interpolated(),
            TokenKind::Match => self.parse_match(true),
            TokenKind::LBrace => {
                let (fields, span) = self.parse_record_fields()?;
                Ok(Expression::Record { fields, span })
            }
            TokenKind::LBracket => {
                let (elements, span) = self.parse_list_elements()?;
                Ok(Expression::List { elements, span })
            }
            TokenKind::Ident(_) | TokenKind::PathSep => {
                let (root, path, callee_span) = self.parse_rooted_path("in an expression")?;
                if self.check(&TokenKind::LParen) {
                    let (arguments, arguments_span) = self.parse_arguments()?;
                    Ok(Expression::Call {
                        root,
                        callee: path,
                        callee_span,
                        arguments,
                        span: callee_span.merge(arguments_span),
                    })
                } else {
                    Ok(Expression::Name {
                        root,
                        path,
                        span: callee_span,
                    })
                }
            }
            other => Err(self.error_here(format!(
                "expected an expression, found {}",
                other.describe()
            ))),
        }
    }

    /// Parse a string: a plain text literal, or an interpolated string whose
    /// parts are literal text and `@(…)` expressions.
    fn parse_interpolated(&mut self) -> Result<Expression, ParseError> {
        let start = self.current().span.start;
        let mut parts = Vec::new();
        let mut end = start;
        loop {
            match &self.current().kind {
                TokenKind::Text(value) => {
                    let value = value.clone();
                    end = self.advance().end;
                    parts.push(StringPart::Literal(value));
                }
                TokenKind::InterpolationStart => {
                    self.advance();
                    let expression = self.parse_expression()?;
                    end = self
                        .expect(&TokenKind::InterpolationEnd, "to close the interpolation")?
                        .end;
                    parts.push(StringPart::Expression(expression));
                }
                _ => break,
            }
        }
        let span = Span::new(start, end);
        match parts.as_slice() {
            [StringPart::Literal(value)] => Ok(Expression::Text {
                value: value.clone(),
                span,
            }),
            _ => Ok(Expression::Interpolated { parts, span }),
        }
    }
}
