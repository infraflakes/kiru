//! Match arms: an arm is an atom, and two equal arms are a duplicate.
//!
//! A match arm references the value it compares; it does not compute one. So an
//! arm is an atom — a text or map literal, a name, or a field path —
//! never a call, a concatenation, or an interpolation. That keeps matching free
//! of side effects and makes duplicate detection a comparison of atoms.

use crate::model::Type;
use crate::model::{Expression, MatchArm, Pattern};
use crate::syntax::UnaryOp;

/// The value of a negative integer literal such as `-1`, which the parser makes
/// a `Negate` over an `Int`. A match arm may compare against one.
fn negative_int(expression: &Expression) -> Option<i64> {
    match expression {
        Expression::Unary {
            op: UnaryOp::Negate,
            operand,
            ..
        } => match &**operand {
            Expression::Int { value, .. } => Some(-*value),
            _ => None,
        },
        _ => None,
    }
}

/// Whether a match over a boolean has an arm for both `true` and `false`, so it
/// needs no `_` arm. Only a literal arm covers a value; a name or a field path
/// does not.
pub(super) fn covers_booleans(subject_type: &Type, cases: &[MatchArm]) -> bool {
    if subject_type != &Type::Bool {
        return false;
    }
    let mut covers_true = false;
    let mut covers_false = false;
    for arm in cases {
        if let Pattern::Value(Expression::Bool { value, .. }) = &arm.pattern {
            match value {
                true => covers_true = true,
                false => covers_false = true,
            }
        }
    }
    covers_true && covers_false
}

/// Whether an expression is an atom: a literal, a name, or a field path. A
/// map literal is an atom when every field value is one.
pub(super) fn is_atom(expression: &Expression) -> bool {
    match expression {
        Expression::Text { .. }
        | Expression::Bool { .. }
        | Expression::Int { .. }
        | Expression::Reference { .. } => true,
        Expression::Field { target, .. } => is_atom(target),
        Expression::Map { fields, .. } => fields.iter().all(|field| is_atom(&field.value)),
        Expression::Unary { .. } => negative_int(expression).is_some(),
        _ => false,
    }
}

/// Whether two atoms are the same: equal literal value, the same declaration,
/// or the same field path. Field order does not matter.
/// A repeated key is rejected before an atom is compared, so every map here
/// has distinct fields.
pub(super) fn atoms_equal(left: &Expression, right: &Expression) -> bool {
    match (left, right) {
        (Expression::Text { value: a, .. }, Expression::Text { value: b, .. }) => a == b,
        (Expression::Bool { value: a, .. }, Expression::Bool { value: b, .. }) => a == b,
        (Expression::Int { value: a, .. }, Expression::Int { value: b, .. }) => a == b,
        (
            Expression::Reference { declaration: a, .. },
            Expression::Reference { declaration: b, .. },
        ) => a == b,
        (
            Expression::Field {
                target: a,
                name: a_name,
                ..
            },
            Expression::Field {
                target: b,
                name: b_name,
                ..
            },
        ) => a_name == b_name && atoms_equal(a, b),
        (Expression::Map { fields: a, .. }, Expression::Map { fields: b, .. }) => {
            a.len() == b.len()
                && a.iter().all(|field| {
                    b.iter().any(|other| {
                        other.name == field.name && atoms_equal(&field.value, &other.value)
                    })
                })
        }
        (Expression::Unary { .. }, Expression::Unary { .. }) => {
            negative_int(left) == negative_int(right)
        }
        _ => false,
    }
}

/// The diagnostic for a repeated match arm; a literal text is quoted.
pub(super) fn duplicate_pattern_message(pattern: &Expression) -> String {
    match pattern {
        Expression::Text { value, .. } => format!("duplicate match pattern `{value}`"),
        Expression::Bool { value, .. } => format!("duplicate match pattern `{value}`"),
        Expression::Int { value, .. } => format!("duplicate match pattern `{value}`"),
        Expression::Unary { .. } => match negative_int(pattern) {
            Some(value) => format!("duplicate match pattern `{value}`"),
            None => "duplicate match pattern".to_owned(),
        },
        _ => "duplicate match pattern".to_owned(),
    }
}
