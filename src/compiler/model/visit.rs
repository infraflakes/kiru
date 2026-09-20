//! One structural traversal of linked expressions and statements.
//!
//! Every pass that visits the model without interpreting it uses this module:
//! reachability collection, declaration remapping, and structural
//! verification. Recursion over every node kind lives here, so adding a node
//! kind is a change to this file alone. A visitor observes each node before
//! its children are walked, which lets a mutable visitor rewrite a node's own
//! edges before its subtree is visited.

use crate::compiler::{Expression, Statement};

/// A read-only walk over a body. The default methods do nothing, so a pass
/// overrides only the node kinds it cares about.
pub(crate) trait Visitor {
    fn expression(&mut self, _expression: &Expression) {}
    fn statement(&mut self, _statement: &Statement) {}
}

/// A rewriting walk over a body. The default methods do nothing, so a pass
/// overrides only the node kinds it changes.
pub(crate) trait VisitorMut {
    fn expression(&mut self, _expression: &mut Expression) {}
    fn statement(&mut self, _statement: &mut Statement) {}
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

/// Walk the children of one statement: its expressions, its switch subjects,
/// case patterns, and case and default bodies, and its defer body.
fn walk_statement(visitor: &mut impl Visitor, statement: &Statement) {
    match statement {
        Statement::Bind { value, .. }
        | Statement::Assign { value, .. }
        | Statement::Expression(value)
        | Statement::Return { value, .. } => walk_expression(visitor, value),
        Statement::Switch {
            subject,
            cases,
            default,
            ..
        } => {
            walk_expression(visitor, subject);
            for case in cases {
                walk_expression(visitor, &case.pattern);
                walk_statements(visitor, &case.body);
            }
            if let Some(default) = default {
                walk_statements(visitor, default);
            }
        }
        Statement::Defer { body, .. } => walk_statements(visitor, body),
    }
}

/// Rewrite the children of one statement, mirroring `walk_statement`.
fn walk_statement_mut(visitor: &mut impl VisitorMut, statement: &mut Statement) {
    match statement {
        Statement::Bind { value, .. }
        | Statement::Assign { value, .. }
        | Statement::Expression(value)
        | Statement::Return { value, .. } => walk_expression_mut(visitor, value),
        Statement::Switch {
            subject,
            cases,
            default,
            ..
        } => {
            walk_expression_mut(visitor, subject);
            for case in cases {
                walk_expression_mut(visitor, &mut case.pattern);
                walk_statements_mut(visitor, &mut case.body);
            }
            if let Some(default) = default {
                walk_statements_mut(visitor, default);
            }
        }
        Statement::Defer { body, .. } => walk_statements_mut(visitor, body),
    }
}

/// Walk the children of one expression: records, call and method arguments,
/// field and method targets, and the operands of an addition.
fn walk_expression_children(visitor: &mut impl Visitor, expression: &Expression) {
    match expression {
        Expression::Text { .. } | Expression::Reference { .. } => {}
        Expression::Record { fields, .. } => {
            for field in fields {
                walk_expression(visitor, &field.value);
            }
        }
        Expression::Call { arguments, .. } => {
            for argument in arguments {
                walk_expression(visitor, argument);
            }
        }
        Expression::Field { target, .. } => walk_expression(visitor, target),
        Expression::Method {
            target, arguments, ..
        } => {
            walk_expression(visitor, target);
            for argument in arguments {
                walk_expression(visitor, argument);
            }
        }
        Expression::Add { left, right, .. } => {
            walk_expression(visitor, left);
            walk_expression(visitor, right);
        }
    }
}

/// Rewrite the children of one expression, mirroring `walk_expression_children`.
fn walk_expression_children_mut(visitor: &mut impl VisitorMut, expression: &mut Expression) {
    match expression {
        Expression::Text { .. } | Expression::Reference { .. } => {}
        Expression::Record { fields, .. } => {
            for field in fields {
                walk_expression_mut(visitor, &mut field.value);
            }
        }
        Expression::Call { arguments, .. } => {
            for argument in arguments {
                walk_expression_mut(visitor, argument);
            }
        }
        Expression::Field { target, .. } => walk_expression_mut(visitor, target),
        Expression::Method {
            target, arguments, ..
        } => {
            walk_expression_mut(visitor, target);
            for argument in arguments {
                walk_expression_mut(visitor, argument);
            }
        }
        Expression::Add { left, right, .. } => {
            walk_expression_mut(visitor, left);
            walk_expression_mut(visitor, right);
        }
    }
}
