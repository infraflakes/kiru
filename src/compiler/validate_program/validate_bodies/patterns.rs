//! Match arms: an arm is an atom, and two equal arms are a duplicate.
//!
//! A match arm references the value it compares; it does not compute one. So an
//! arm is an atom — a text, record, or list literal, a name, or a field path —
//! never a call, a concatenation, or an interpolation. That keeps matching free
//! of side effects and makes duplicate detection a comparison of atoms.

use crate::model::{Expression, Field};

/// Whether an expression is an atom: a literal, a name, or a field path. A
/// record or list literal is an atom when every field or element is one.
pub(super) fn is_atom(expression: &Expression) -> bool {
    match expression {
        Expression::Text { .. } | Expression::Reference { .. } => true,
        Expression::Field { target, .. } => is_atom(target),
        Expression::Record { fields, .. } => fields.iter().all(|field| is_atom(&field.value)),
        Expression::List { elements, .. } => elements.iter().all(is_atom),
        _ => false,
    }
}

/// Whether two atoms are the same: equal literal text, the same declaration, or
/// the same field path. A record is a map, so field order does not matter and a
/// repeated key collapses to its last value on both sides.
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
        (Expression::Record { fields: a, .. }, Expression::Record { fields: b, .. }) => {
            let a = normalized_fields(a);
            let b = normalized_fields(b);
            a.len() == b.len()
                && a.iter().all(|(name, value)| {
                    b.iter().any(|(other, other_value)| {
                        other == name && atoms_equal(value, other_value)
                    })
                })
        }
        (Expression::List { elements: a, .. }, Expression::List { elements: b, .. }) => {
            a.len() == b.len() && a.iter().zip(b).all(|(a, b)| atoms_equal(a, b))
        }
        _ => false,
    }
}

/// The fields of a record atom with a repeated key collapsed to its last value,
/// so two literals that denote the same record compare equal.
fn normalized_fields(fields: &[Field]) -> Vec<(&str, &Expression)> {
    let mut result: Vec<(&str, &Expression)> = Vec::new();
    for field in fields {
        match result
            .iter_mut()
            .find(|(name, _)| *name == field.name.as_str())
        {
            Some(entry) => entry.1 = &field.value,
            None => result.push((&field.name, &field.value)),
        }
    }
    result
}

/// The diagnostic for a repeated match arm; a literal text is quoted.
pub(super) fn duplicate_pattern_message(pattern: &Expression) -> String {
    match pattern {
        Expression::Text { value, .. } => format!("duplicate match pattern `{value}`"),
        _ => "duplicate match pattern".to_owned(),
    }
}
