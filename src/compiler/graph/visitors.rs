//! One structural traversal of linked expressions and statements.
//!
//! The recursion over every node kind lives here: `walk_statements`,
//! `walk_expression`, and `walk_pattern` descend into every child, so a pass
//! that visits the model without interpreting it names only the kinds it acts
//! on and this module carries it to the rest. Reachability collection,
//! closure capture collection, module-value ordering, and structural
//! verification read a body through this module.
//!
//! Each trait names one method per node kind and every method is required, with
//! no default body: a new kind is a new required method, so every visitor fails
//! to compile until it names the kind, and no kind can pass a visitor silently.
//!
//! A pass that rewrites or builds nodes cannot use a read-only visitor, so it
//! keeps its own match over the kinds it produces. They are:
//!
//! - `derive::resolve::body_linking`, which builds model nodes from the parsed
//!   syntax tree.
//! - `derive::resolve::binding_linking`, which builds a local binding node and
//!   a variant pattern from a parsed pattern.
//! - `lower_expressions`, which builds instructions from the model and gives
//!   every expression a register.
//!
//! The checker in `derive::check` also reads the model without rewriting
//! it, but it interprets each kind — it derives a type or a flow — so it keeps
//! its own recursion instead of this observer.

use super::expressions::{Expression, MatchBody, Pattern, Statement, StringPart};

/// A read-only walk over a body. One method per node kind, every one required.
pub(crate) trait Visitor {
    fn text(&mut self, expression: &Expression);
    fn map(&mut self, expression: &Expression);
    fn boolean(&mut self, expression: &Expression);
    fn integer(&mut self, expression: &Expression);
    fn variant(&mut self, expression: &Expression);
    fn interpolated(&mut self, expression: &Expression);
    fn reference(&mut self, expression: &Expression);
    fn call(&mut self, expression: &Expression);
    /// A family callee: a call whose bound function the check selects. It
    /// carries no graph edge; the check records its target in `Facts`, so only
    /// a pass holding the facts acts on it.
    fn method(&mut self, expression: &Expression);
    fn field(&mut self, expression: &Expression);
    fn binary(&mut self, expression: &Expression);
    fn unary(&mut self, expression: &Expression);
    fn match_expression(&mut self, expression: &Expression);
    fn function(&mut self, expression: &Expression);

    fn bind(&mut self, statement: &Statement);
    fn assign(&mut self, statement: &Statement);
    fn field_assign(&mut self, statement: &Statement);
    fn expression_statement(&mut self, statement: &Statement);
    fn return_statement(&mut self, statement: &Statement);
    fn panic(&mut self, statement: &Statement);
    fn asynchronous(&mut self, statement: &Statement);
    fn wait(&mut self, statement: &Statement);
    fn forever(&mut self, statement: &Statement);
    fn break_statement(&mut self, statement: &Statement);

    fn value_pattern(&mut self, pattern: &Pattern);
    fn variant_pattern(&mut self, pattern: &Pattern);

    fn expression_body(&mut self, body: &MatchBody);
    fn block_body(&mut self, body: &MatchBody);

    /// Whether the walk descends into a function literal's body. A literal is
    /// evaluated where it is written to make a closure value; its body runs only
    /// when the value is called, so a pass that reads what an initializer
    /// evaluates up front stops here and a pass that reads all code continues.
    fn enters_deferred_body(&self) -> bool;
}

/// Visit one match pattern: its bindings, then its value expression.
pub(crate) fn walk_pattern(visitor: &mut impl Visitor, pattern: &Pattern) {
    observe_pattern(visitor, pattern);
    match pattern {
        Pattern::Value(expression) => walk_expression(visitor, expression),
        Pattern::Variant { .. } => {}
    }
}

/// Observe one match pattern at the method for its kind.
fn observe_pattern(visitor: &mut impl Visitor, pattern: &Pattern) {
    match pattern {
        Pattern::Value(_) => visitor.value_pattern(pattern),
        Pattern::Variant { .. } => visitor.variant_pattern(pattern),
    }
}

/// Visit every statement and, recursively, every expression under it.
pub(crate) fn walk_statements(visitor: &mut impl Visitor, statements: &[Statement]) {
    for statement in statements {
        observe_statement(visitor, statement);
        walk_statement_children(visitor, statement);
    }
}

/// Visit every expression and, recursively, every expression under it.
pub(crate) fn walk_expression(visitor: &mut impl Visitor, expression: &Expression) {
    observe_expression(visitor, expression);
    walk_expression_children(visitor, expression);
}

/// Observe one expression at the method for its kind.
fn observe_expression(visitor: &mut impl Visitor, expression: &Expression) {
    match expression {
        Expression::Text { .. } => visitor.text(expression),
        Expression::Map { .. } => visitor.map(expression),
        Expression::Bool { .. } => visitor.boolean(expression),
        Expression::Int { .. } => visitor.integer(expression),
        Expression::Variant { .. } => visitor.variant(expression),
        Expression::Interpolated { .. } => visitor.interpolated(expression),
        Expression::Reference { .. } => visitor.reference(expression),
        Expression::Call { .. } => visitor.call(expression),
        Expression::Method { .. } => visitor.method(expression),
        Expression::Field { .. } => visitor.field(expression),
        Expression::Binary { .. } => visitor.binary(expression),
        Expression::Unary { .. } => visitor.unary(expression),
        Expression::Match { .. } => visitor.match_expression(expression),
        Expression::Function(_) => visitor.function(expression),
    }
}

/// Observe one statement at the method for its kind.
fn observe_statement(visitor: &mut impl Visitor, statement: &Statement) {
    match statement {
        Statement::Bind { .. } => visitor.bind(statement),
        Statement::Assign { .. } => visitor.assign(statement),
        Statement::FieldAssign { .. } => visitor.field_assign(statement),
        Statement::Expression(_) => visitor.expression_statement(statement),
        Statement::Return { .. } => visitor.return_statement(statement),
        Statement::Panic { .. } => visitor.panic(statement),
        Statement::Async { .. } => visitor.asynchronous(statement),
        Statement::Wait { .. } => visitor.wait(statement),
        Statement::Forever { .. } => visitor.forever(statement),
        Statement::Break { .. } => visitor.break_statement(statement),
    }
}

/// Walk the children of one statement: its expressions, its match subject,
/// its arm patterns and bodies.
fn walk_statement_children(visitor: &mut impl Visitor, statement: &Statement) {
    match statement {
        Statement::Bind { value, .. }
        | Statement::Assign { value, .. }
        | Statement::FieldAssign { value, .. }
        | Statement::Expression(value) => walk_expression(visitor, value),
        Statement::Return { value, .. } => {
            if let Some(value) = value {
                walk_expression(visitor, value);
            }
        }
        Statement::Panic { .. } => {}
        Statement::Async { call, .. } => walk_expression(visitor, call),
        Statement::Wait { .. } => {}
        Statement::Forever { body, .. } => walk_statements(visitor, body),
        Statement::Break { .. } => {}
    }
}

/// Walk the children of one expression: maps, call arguments, field
/// targets, and the operands of an addition.
fn walk_expression_children(visitor: &mut impl Visitor, expression: &Expression) {
    match expression {
        Expression::Text { .. }
        | Expression::Bool { .. }
        | Expression::Int { .. }
        | Expression::Reference { .. }
        | Expression::Method { .. } => {}
        Expression::Map { fields, .. } => {
            for field in fields {
                walk_expression(visitor, &field.value);
            }
        }
        Expression::Interpolated { parts, .. } => {
            for part in parts {
                match part {
                    StringPart::Literal(_) => {}
                    StringPart::Expression(expression) => walk_expression(visitor, expression),
                }
            }
        }
        Expression::Call {
            callee, arguments, ..
        } => {
            walk_expression(visitor, callee);
            for argument in arguments {
                walk_expression(visitor, argument);
            }
        }
        Expression::Field { target, .. } => walk_expression(visitor, target),
        Expression::Variant { payload, .. } => {
            for element in payload {
                walk_expression(visitor, element);
            }
        }
        Expression::Binary { left, right, .. } => {
            walk_expression(visitor, left);
            walk_expression(visitor, right);
        }
        Expression::Unary { operand, .. } => walk_expression(visitor, operand),
        Expression::Function(function) => {
            if visitor.enters_deferred_body() {
                walk_statements(visitor, &function.body);
            }
        }
        Expression::Match {
            subject,
            cases,
            default,
            ..
        } => {
            walk_expression(visitor, subject);
            for arm in cases {
                walk_pattern(visitor, &arm.pattern);
                walk_match_body(visitor, &arm.body);
            }
            if let Some(default) = default {
                walk_match_body(visitor, default);
            }
        }
    }
}

/// Visit one match arm body: its expression, or its statements.
fn walk_match_body(visitor: &mut impl Visitor, body: &MatchBody) {
    observe_match_body(visitor, body);
    match body {
        MatchBody::Expression(expression) => walk_expression(visitor, expression),
        MatchBody::Block(statements) => walk_statements(visitor, statements),
    }
}

/// Observe one match arm body at the method for its kind.
fn observe_match_body(visitor: &mut impl Visitor, body: &MatchBody) {
    match body {
        MatchBody::Expression(_) => visitor.expression_body(body),
        MatchBody::Block(_) => visitor.block_body(body),
    }
}

#[cfg(test)]
mod tests {
    use crate::compiler::graph::{Expression, MatchBody, Pattern, Statement};
    use crate::syntax::{BinaryOp, Span};

    use super::{Visitor, walk_expression};

    /// Records the kind of each expression the walk observes.
    #[derive(Default)]
    struct KindRecorder {
        kinds: Vec<&'static str>,
    }

    impl Visitor for KindRecorder {
        fn text(&mut self, _: &Expression) {
            self.kinds.push("text");
        }
        fn integer(&mut self, _: &Expression) {
            self.kinds.push("integer");
        }
        fn binary(&mut self, _: &Expression) {
            self.kinds.push("binary");
        }

        fn map(&mut self, _: &Expression) {}
        fn boolean(&mut self, _: &Expression) {}
        fn variant(&mut self, _: &Expression) {}
        fn interpolated(&mut self, _: &Expression) {}
        fn reference(&mut self, _: &Expression) {}
        fn call(&mut self, _: &Expression) {}
        fn method(&mut self, _: &Expression) {}
        fn field(&mut self, _: &Expression) {}
        fn unary(&mut self, _: &Expression) {}
        fn match_expression(&mut self, _: &Expression) {}
        fn function(&mut self, _: &Expression) {}
        fn bind(&mut self, _: &Statement) {}
        fn assign(&mut self, _: &Statement) {}
        fn field_assign(&mut self, _: &Statement) {}
        fn expression_statement(&mut self, _: &Statement) {}
        fn return_statement(&mut self, _: &Statement) {}
        fn panic(&mut self, _: &Statement) {}
        fn asynchronous(&mut self, _: &Statement) {}
        fn wait(&mut self, _: &Statement) {}
        fn forever(&mut self, _: &Statement) {}
        fn break_statement(&mut self, _: &Statement) {}
        fn value_pattern(&mut self, _: &Pattern) {}
        fn variant_pattern(&mut self, _: &Pattern) {}
        fn expression_body(&mut self, _: &MatchBody) {}
        fn block_body(&mut self, _: &MatchBody) {}

        fn enters_deferred_body(&self) -> bool {
            true
        }
    }

    #[test]
    fn the_walk_observes_each_expression_at_its_own_method() {
        let expression = Expression::Binary {
            op: BinaryOp::Add,
            left: Box::new(Expression::Int {
                value: 1,
                span: Span::new(0, 1),
            }),
            right: Box::new(Expression::Text {
                value: "x".to_owned(),
                span: Span::new(2, 3),
            }),
            span: Span::new(0, 3),
        };
        let mut recorder = KindRecorder::default();
        walk_expression(&mut recorder, &expression);
        assert_eq!(recorder.kinds, ["binary", "integer", "text"]);
    }
}
