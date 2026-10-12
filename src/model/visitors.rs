//! One structural traversal of linked expressions and statements.
//!
//! Every pass that visits the model without interpreting it uses this module:
//! reachability collection, declaration remapping, and structural
//! verification. Recursion over every node kind lives here, so adding a node
//! kind is a change to this file alone. A visitor observes each node before
//! its children are walked, which lets a mutable visitor rewrite a node's own
//! edges before its subtree is visited.
//!
//! Both visitor methods are required, so each pass names every node kind it
//! visits and Vec every variant explicitly. A new node kind is then a
//! compile error in every pass, not a silently missed node.

use super::body::{Expression, MatchBody, Pattern, Statement, StringPart};

/// A read-only walk over a body.
pub(crate) trait Visitor {
    fn expression(&mut self, expression: &Expression);
    fn statement(&mut self, statement: &Statement);
    /// Observe one match pattern: its value expression and its bindings.
    fn pattern(&mut self, pattern: &Pattern);
}

/// A rewriting walk over a body.
pub(crate) trait VisitorMut {
    fn expression(&mut self, expression: &mut Expression);
    fn statement(&mut self, statement: &mut Statement);
    /// Rewrite one match pattern's value expression and bindings.
    fn pattern(&mut self, pattern: &mut Pattern);
}

/// Visit one match pattern: its bindings, then its value expression.
pub(crate) fn walk_pattern(visitor: &mut impl Visitor, pattern: &Pattern) {
    visitor.pattern(pattern);
    if let Pattern::Value(expression) = pattern {
        walk_expression(visitor, expression);
    }
}

/// Rewrite one match pattern: its bindings, then its value expression.
pub(crate) fn walk_pattern_mut(visitor: &mut impl VisitorMut, pattern: &mut Pattern) {
    visitor.pattern(pattern);
    if let Pattern::Value(expression) = pattern {
        walk_expression_mut(visitor, expression);
    }
}

/// Visit every statement and, recursively, every expression under it.
pub(crate) fn walk_statements(visitor: &mut impl Visitor, statements: &[Statement]) {
    for statement in statements {
        visitor.statement(statement);
        walk_statement(visitor, statement);
    }
}

/// Visit every expression and, recursively, every expression under it.
pub(crate) fn walk_expression(visitor: &mut impl Visitor, expression: &Expression) {
    visitor.expression(expression);
    walk_expression_children(visitor, expression);
}

/// Rewrite every statement and, recursively, every expression under it.
pub(crate) fn walk_statements_mut(visitor: &mut impl VisitorMut, statements: &mut [Statement]) {
    for statement in statements {
        visitor.statement(statement);
        walk_statement_mut(visitor, statement);
    }
}

/// Rewrite every expression and, recursively, every expression under it.
pub(crate) fn walk_expression_mut(visitor: &mut impl VisitorMut, expression: &mut Expression) {
    visitor.expression(expression);
    walk_expression_children_mut(visitor, expression);
}

/// Walk the children of one statement: its expressions, its match subject,
/// its arm patterns and bodies.
fn walk_statement(visitor: &mut impl Visitor, statement: &Statement) {
    match statement {
        Statement::Bind { value, .. }
        | Statement::Assign { value, .. }
        | Statement::FieldAssign { value, .. }
        | Statement::Expression(value) => walk_expression(visitor, value),
        Statement::IndexAssign { index, value, .. } => {
            walk_expression(visitor, index);
            walk_expression(visitor, value);
        }
        Statement::Return { value, .. } => {
            if let Some(value) = value {
                walk_expression(visitor, value);
            }
        }
        Statement::Panic { .. } => {}
        Statement::Async { call, .. } => walk_expression(visitor, call),
        Statement::Wait { .. } => {}
        Statement::ForEach { iterable, body, .. } => {
            walk_expression(visitor, iterable);
            walk_statements(visitor, body);
        }
        Statement::Forever { body, .. } => walk_statements(visitor, body),
        Statement::Break { .. } => {}
    }
}

/// Rewrite the children of one statement, mirroring `walk_statement`.
fn walk_statement_mut(visitor: &mut impl VisitorMut, statement: &mut Statement) {
    match statement {
        Statement::Bind { value, .. }
        | Statement::Assign { value, .. }
        | Statement::FieldAssign { value, .. }
        | Statement::Expression(value) => walk_expression_mut(visitor, value),
        Statement::IndexAssign { index, value, .. } => {
            walk_expression_mut(visitor, index);
            walk_expression_mut(visitor, value);
        }
        Statement::Return { value, .. } => {
            if let Some(value) = value {
                walk_expression_mut(visitor, value);
            }
        }
        Statement::Panic { .. } => {}
        Statement::Async { call, .. } => walk_expression_mut(visitor, call),
        Statement::Wait { .. } => {}
        Statement::ForEach { iterable, body, .. } => {
            walk_expression_mut(visitor, iterable);
            walk_statements_mut(visitor, body);
        }
        Statement::Forever { body, .. } => walk_statements_mut(visitor, body),
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
        | Expression::Reference { .. } => {}
        Expression::Map { fields, .. } => {
            for field in fields {
                walk_expression(visitor, &field.value);
            }
        }
        Expression::Vec { elements, .. } => {
            for element in elements {
                walk_expression(visitor, element);
            }
        }
        Expression::Interpolated { parts, .. } => {
            for part in parts {
                if let StringPart::Expression(expression) = part {
                    walk_expression(visitor, expression);
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
        Expression::Index { target, index, .. } => {
            walk_expression(visitor, target);
            walk_expression(visitor, index);
        }
        Expression::Binary { left, right, .. } => {
            walk_expression(visitor, left);
            walk_expression(visitor, right);
        }
        Expression::Unary { operand, .. } => walk_expression(visitor, operand),
        Expression::Function(function) => walk_statements(visitor, &function.body),
        Expression::Match {
            subject,
            cases,
            default,
            ..
        } => {
            walk_expression(visitor, subject);
            for arm in cases {
                walk_pattern(visitor, &arm.pattern);
                match &arm.body {
                    MatchBody::Expression(expression) => walk_expression(visitor, expression),
                    MatchBody::Block(statements) => walk_statements(visitor, statements),
                }
            }
            if let Some(default) = default {
                match default {
                    MatchBody::Expression(expression) => walk_expression(visitor, expression),
                    MatchBody::Block(statements) => walk_statements(visitor, statements),
                }
            }
        }
    }
}

/// Rewrite the children of one expression, mirroring `walk_expression_children`.
fn walk_expression_children_mut(visitor: &mut impl VisitorMut, expression: &mut Expression) {
    match expression {
        Expression::Text { .. }
        | Expression::Bool { .. }
        | Expression::Int { .. }
        | Expression::Reference { .. } => {}
        Expression::Map { fields, .. } => {
            for field in fields {
                walk_expression_mut(visitor, &mut field.value);
            }
        }
        Expression::Vec { elements, .. } => {
            for element in elements {
                walk_expression_mut(visitor, element);
            }
        }
        Expression::Interpolated { parts, .. } => {
            for part in parts {
                if let StringPart::Expression(expression) = part {
                    walk_expression_mut(visitor, expression);
                }
            }
        }
        Expression::Call {
            callee, arguments, ..
        } => {
            walk_expression_mut(visitor, callee);
            for argument in arguments {
                walk_expression_mut(visitor, argument);
            }
        }
        Expression::Field { target, .. } => walk_expression_mut(visitor, target),
        Expression::Variant { payload, .. } => {
            for element in payload {
                walk_expression_mut(visitor, element);
            }
        }
        Expression::Index { target, index, .. } => {
            walk_expression_mut(visitor, target);
            walk_expression_mut(visitor, index);
        }
        Expression::Binary { left, right, .. } => {
            walk_expression_mut(visitor, left);
            walk_expression_mut(visitor, right);
        }
        Expression::Unary { operand, .. } => walk_expression_mut(visitor, operand),
        Expression::Function(function) => walk_statements_mut(visitor, &mut function.body),
        Expression::Match {
            subject,
            cases,
            default,
            ..
        } => {
            walk_expression_mut(visitor, subject);
            for arm in cases {
                walk_pattern_mut(visitor, &mut arm.pattern);
                match &mut arm.body {
                    MatchBody::Expression(expression) => walk_expression_mut(visitor, expression),
                    MatchBody::Block(statements) => walk_statements_mut(visitor, statements),
                }
            }
            if let Some(default) = default {
                match default {
                    MatchBody::Expression(expression) => walk_expression_mut(visitor, expression),
                    MatchBody::Block(statements) => walk_statements_mut(visitor, statements),
                }
            }
        }
    }
}
