//! Match arms: an arm is a text atom, and two equal arms are a duplicate.
//!
//! A match arm references the text it compares; it does not compute one. So an
//! arm is an atom — a text literal, a name, or a field path — never a call, a
//! concatenation, or a record or list literal. That keeps matching free of
//! side effects and makes duplicate detection a comparison of atoms.

use crate::model::Expression;

/// Whether an expression is an atom: a text literal, a name, or a field path.
pub(super) fn is_atom(expression: &Expression) -> bool {
    match expression {
        Expression::Text { .. } | Expression::Reference { .. } => true,
        Expression::Field { target, .. } => is_atom(target),
        _ => false,
    }
}

/// Whether two atoms are the same: equal literal text, the same declaration, or
/// the same field path.
pub(super) fn atoms_equal(left: &Expression, right: &Expression) -> bool {
    match (left, right) {
        (Expression::Text { value: a, .. }, Expression::Text { value: b, .. }) => a == b,
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
        _ => false,
    }
}

/// The diagnostic for a repeated match arm; a literal text is quoted.
pub(super) fn duplicate_pattern_message(pattern: &Expression) -> String {
    match pattern {
        Expression::Text { value, .. } => format!("duplicate match pattern `{value}`"),
        _ => "duplicate match pattern".to_owned(),
    }
}
