//! Expression grammar: precedence-climbing binary expressions, prefix
//! operators, postfix field access, and primary expressions.

use crate::syntax::Span;
use crate::syntax::ast::{BinaryOp, Expression, StringPart, UnaryOp};
use crate::syntax::token::TokenKind;

use super::{ParseError, Parser};

/// The binary operator a token writes, or `None` when the token is not one.
fn binary_operator(kind: &TokenKind) -> Option<BinaryOp> {
    match kind {
        TokenKind::Plus => Some(BinaryOp::Add),
        TokenKind::Minus => Some(BinaryOp::Subtract),
        TokenKind::Star => Some(BinaryOp::Multiply),
        TokenKind::Slash => Some(BinaryOp::Divide),
        TokenKind::Percent => Some(BinaryOp::Remainder),
        TokenKind::Less => Some(BinaryOp::Less),
        TokenKind::LessEq => Some(BinaryOp::LessEqual),
        TokenKind::Greater => Some(BinaryOp::Greater),
        TokenKind::GreaterEq => Some(BinaryOp::GreaterEqual),
        TokenKind::EqEq => Some(BinaryOp::Equal),
        TokenKind::NotEq => Some(BinaryOp::NotEqual),
        TokenKind::AndAnd => Some(BinaryOp::And),
        TokenKind::OrOr => Some(BinaryOp::Or),
        _ => None,
    }
}

/// The binding power of a binary operator: a higher number binds tighter.
fn precedence(operator: BinaryOp) -> u8 {
    match operator {
        BinaryOp::Or => 1,
        BinaryOp::And => 2,
        BinaryOp::Equal | BinaryOp::NotEqual => 3,
        BinaryOp::Less | BinaryOp::LessEqual | BinaryOp::Greater | BinaryOp::GreaterEqual => 4,
        BinaryOp::Add | BinaryOp::Subtract => 5,
        BinaryOp::Multiply | BinaryOp::Divide | BinaryOp::Remainder => 6,
    }
}

/// The prefix operator a token writes, or `None` when the token is not one.
fn unary_operator(kind: &TokenKind) -> Option<UnaryOp> {
    match kind {
        TokenKind::Minus => Some(UnaryOp::Negate),
        TokenKind::Bang => Some(UnaryOp::Not),
        _ => None,
    }
}

impl Parser {
    /// Parse an expression by precedence climbing.
    pub(super) fn parse_expression(&mut self) -> Result<Expression, ParseError> {
        self.enter_nesting()?;
        let expression = self.parse_binary(0)?;
        self.leave_nesting();
        Ok(expression)
    }

    /// Parse a binary expression: a prefix operand, then every following
    /// operator that binds at least as tightly as `minimum`, folded left to
    /// right. A tighter operator is consumed by the recursive call, so the tree
    /// follows precedence; a same-precedence operator is left to this loop, so
    /// it groups left.
    fn parse_binary(&mut self, minimum: u8) -> Result<Expression, ParseError> {
        let mut left = self.parse_unary()?;
        while let Some(operator) = binary_operator(&self.current().kind) {
            let binding = precedence(operator);
            if binding < minimum {
                break;
            }
            self.advance();
            let right = self.parse_binary(binding + 1)?;
            let span = left.span().merge(right.span());
            left = Expression::Binary {
                op: operator,
                left: Box::new(left),
                right: Box::new(right),
                span,
            };
        }
        Ok(left)
    }

    /// Parse a prefix operator and its operand, or a postfix expression when
    /// the cursor is not on one.
    fn parse_unary(&mut self) -> Result<Expression, ParseError> {
        let Some(operator) = unary_operator(&self.current().kind) else {
            return self.parse_postfix();
        };
        let start = self.advance().start;
        let operand = self.parse_unary()?;
        let span = Span::new(start, operand.span().end);
        Ok(Expression::Unary {
            op: operator,
            operand: Box::new(operand),
            span,
        })
    }

    /// Parse a primary expression followed by any field accesses or method
    /// calls.
    fn parse_postfix(&mut self) -> Result<Expression, ParseError> {
        let mut expression = self.parse_primary()?;
        loop {
            if self.eat(&TokenKind::Dot) {
                let (name, name_span) = self.expect_identifier("after `.`")?;
                if self.check(&TokenKind::LParen) {
                    let (arguments, arguments_span) = self.parse_arguments()?;
                    let span = expression.span().merge(arguments_span);
                    expression = Expression::MethodCall {
                        receiver: Box::new(expression),
                        method: name,
                        method_span: name_span,
                        arguments,
                        span,
                    };
                } else {
                    let span = expression.span().merge(name_span);
                    expression = Expression::Field {
                        target: Box::new(expression),
                        name,
                        name_span,
                        span,
                    };
                }
            } else {
                break;
            }
        }
        Ok(expression)
    }

    /// Parse a literal, a map, a name, or a call.
    fn parse_primary(&mut self) -> Result<Expression, ParseError> {
        if let TokenKind::Int(value) = self.current().kind {
            let span = self.advance();
            return Ok(Expression::Int { value, span });
        }
        if let TokenKind::True = self.current().kind {
            let span = self.advance();
            return Ok(Expression::Bool { value: true, span });
        }
        if let TokenKind::False = self.current().kind {
            let span = self.advance();
            return Ok(Expression::Bool { value: false, span });
        }
        match &self.current().kind {
            TokenKind::Fn => self.parse_closure(),
            TokenKind::Text(_) | TokenKind::InterpolationStart => self.parse_interpolated(),
            TokenKind::Match => self.parse_match(true),
            TokenKind::LBrace => {
                let (fields, span) = self.parse_rec_fields()?;
                Ok(Expression::Map { fields, span })
            }
            TokenKind::LParen => {
                // A grouped expression carries no node: the parens only steer
                // precedence, which the tree already records.
                self.enter_nesting()?;
                self.advance();
                let inner = self.parse_expression()?;
                self.expect(&TokenKind::RParen, "to close a grouped expression")?;
                self.leave_nesting();
                Ok(inner)
            }
            TokenKind::Ident(_) => {
                let (path, callee_span) = self.parse_path_segments("in an expression")?;
                if self.check(&TokenKind::LParen) {
                    let (arguments, arguments_span) = self.parse_arguments()?;
                    Ok(Expression::Call {
                        callee: path,
                        callee_span,
                        arguments,
                        span: callee_span.merge(arguments_span),
                    })
                } else {
                    Ok(Expression::Name {
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
