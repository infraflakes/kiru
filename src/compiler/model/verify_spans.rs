//! Span verification of the program model.
//!
//! Every span a node carries must be ordered: a span that ends before it
//! starts is a compiler bug, and the assert reports it during development.
//! This is the assert-based half of `verify_program`; the `Result`-based graph
//! checks live in `verify_graph`.

use super::Program;
use super::expressions::{Expression, Field, MatchBody, Pattern, Statement};
use super::visitors::{Visitor, walk_expression};

/// The span checks over every body. Every span is ordered.
pub(super) fn verify_spans(program: &Program) {
    let mut check = SpanCheck;
    for declaration in &program.declarations {
        if let Some(value) = &declaration.value {
            walk_expression(&mut check, value);
        }
    }
}

/// Checks every span is ordered.
struct SpanCheck;

impl SpanCheck {
    /// Assert one expression's own span is ordered.
    fn expression_span(&self, expression: &Expression) {
        let span = expression.span();
        assert!(span.start <= span.end);
    }

    /// Assert one statement's own span is ordered.
    fn statement_span(&self, statement: &Statement) {
        let span = statement.span();
        assert!(span.start <= span.end);
    }
}

impl Visitor for SpanCheck {
    fn text(&mut self, expression: &Expression) {
        self.expression_span(expression);
    }
    fn map(&mut self, expression: &Expression) {
        self.expression_span(expression);
        let Expression::Map { fields, .. } = expression else {
            unreachable!("the walk names a map");
        };
        assert_field_spans(fields);
    }
    fn boolean(&mut self, expression: &Expression) {
        self.expression_span(expression);
    }
    fn integer(&mut self, expression: &Expression) {
        self.expression_span(expression);
    }
    fn variant(&mut self, expression: &Expression) {
        self.expression_span(expression);
    }
    fn interpolated(&mut self, expression: &Expression) {
        self.expression_span(expression);
    }
    fn reference(&mut self, expression: &Expression) {
        self.expression_span(expression);
    }
    fn call(&mut self, expression: &Expression) {
        self.expression_span(expression);
        let Expression::Call { callee_span, .. } = expression else {
            unreachable!("the walk names a call");
        };
        assert!(callee_span.start <= callee_span.end);
    }
    fn field(&mut self, expression: &Expression) {
        self.expression_span(expression);
        let Expression::Field { name_span, .. } = expression else {
            unreachable!("the walk names a field");
        };
        assert!(name_span.start <= name_span.end);
    }
    fn binary(&mut self, expression: &Expression) {
        self.expression_span(expression);
    }
    fn unary(&mut self, expression: &Expression) {
        self.expression_span(expression);
    }
    fn match_expression(&mut self, expression: &Expression) {
        self.expression_span(expression);
    }
    fn function(&mut self, expression: &Expression) {
        self.expression_span(expression);
    }

    fn bind(&mut self, statement: &Statement) {
        self.statement_span(statement);
    }
    fn assign(&mut self, statement: &Statement) {
        self.statement_span(statement);
        let Statement::Assign { name_span, .. } = statement else {
            unreachable!("the walk names an assignment");
        };
        assert!(name_span.start <= name_span.end);
    }
    fn field_assign(&mut self, statement: &Statement) {
        self.statement_span(statement);
        let Statement::FieldAssign { name_span, .. } = statement else {
            unreachable!("the walk names a field assignment");
        };
        assert!(name_span.start <= name_span.end);
    }
    fn expression_statement(&mut self, statement: &Statement) {
        self.statement_span(statement);
    }
    fn return_statement(&mut self, statement: &Statement) {
        self.statement_span(statement);
    }
    fn panic(&mut self, statement: &Statement) {
        self.statement_span(statement);
    }
    fn asynchronous(&mut self, statement: &Statement) {
        self.statement_span(statement);
    }
    fn wait(&mut self, statement: &Statement) {
        self.statement_span(statement);
    }
    fn forever(&mut self, statement: &Statement) {
        self.statement_span(statement);
    }
    fn break_statement(&mut self, statement: &Statement) {
        self.statement_span(statement);
    }

    fn value_pattern(&mut self, _: &Pattern) {}
    fn variant_pattern(&mut self, pattern: &Pattern) {
        let Pattern::Variant { span, .. } = pattern else {
            unreachable!("the walk names a variant pattern");
        };
        assert!(span.start <= span.end);
    }

    fn expression_body(&mut self, _: &MatchBody) {}
    fn block_body(&mut self, _: &MatchBody) {}
}

/// The span checks shared by a map expression and a `rec` declaration.
fn assert_field_spans(fields: &[Field]) {
    for field in fields {
        assert!(field.name_span.start <= field.name_span.end);
        assert!(field.span.start <= field.span.end);
    }
}
