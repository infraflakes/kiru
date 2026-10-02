//! Expression grammar: sums of postfix expressions and primary expressions.

use crate::syntax::ast::Expression;
use crate::syntax::token::TokenKind;

use super::{ParseError, Parser};

impl Parser {
    /// Parse a sum of postfix expressions.
    pub(super) fn parse_expression(&mut self) -> Result<Expression, ParseError> {
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
            TokenKind::Text(_) => {
                let (value, span) = self.expect_text("in an expression")?;
                Ok(Expression::Text { value, span })
            }
            TokenKind::LBrace => {
                let (fields, span) = self.parse_record_fields()?;
                Ok(Expression::Record { fields, span })
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
}
