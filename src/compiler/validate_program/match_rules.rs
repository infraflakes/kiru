//! Match rules: match legality, coverage, and typing.
//!
//! A match arm is an atom — a text or map literal, a name, or a field path —
//! never a call, a concatenation, or an interpolation. That keeps matching free
//! of side effects and makes duplicate detection a comparison of atoms. A match
//! that produces a value must cover every case; a statement match need not.

use crate::compiler::Diagnostic;
use crate::compiler::messages::expected_instead;
use crate::compiler::model::{
    Binding, Expression, MatchArm, MatchBody, Pattern, Type, TypeKind, substitute,
};
use crate::syntax::{Span, UnaryOp};

use super::statement_check::Checker;

/// Whether a match takes an arm for every value: it has a `_` arm, it covers
/// both booleans, or it covers every variant of its enum subject.
pub(super) fn is_exhaustive(
    default: &Option<MatchBody>,
    subject_type: &Type,
    cases: &[MatchArm],
    covered: Option<&[bool]>,
) -> bool {
    default.is_some()
        || covers_booleans(subject_type, cases)
        || covered.is_some_and(|covered| covered.iter().all(|flag| *flag))
}

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
fn covers_booleans(subject_type: &Type, cases: &[MatchArm]) -> bool {
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
fn is_atom(expression: &Expression) -> bool {
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
fn atoms_equal(left: &Expression, right: &Expression) -> bool {
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
fn duplicate_pattern_message(pattern: &Expression) -> String {
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

impl<'a> Checker<'a> {
    /// The name of an optional type for a diagnostic: `no value` when the
    /// expression produced none.
    fn optional_type_name(&self, ty: &Option<Type>) -> String {
        match ty {
            Some(ty) => self.type_name(ty),
            None => "no value".to_owned(),
        }
    }

    /// The type a match compares. Its subject must produce a value: text, a
    /// map, or any other value, never a call that produces none.
    pub(super) fn match_subject(&mut self, subject: &Expression) -> Result<Type, Diagnostic> {
        let Some(ty) = self.expression(subject, None)? else {
            return Err(self.error(subject.span(), "a match subject must have a value"));
        };
        Ok(ty)
    }

    /// The type of a match in expression position: the common type of the arm
    /// expressions, or no type when the arms are blocks. `expected`, when there
    /// is one, is the type the whole match must have; it flows into each arm so
    /// a generic construction or a nominal literal takes its type from the
    /// match. The match must be exhaustive, so it always takes an arm and
    /// always yields a value.
    pub(super) fn match_expression(
        &mut self,
        span: Span,
        subject: &Expression,
        cases: &[MatchArm],
        default: &Option<MatchBody>,
        expected: Option<&Type>,
    ) -> Result<Option<Type>, Diagnostic> {
        let subject_type = self.match_subject(subject)?;
        let mut seen: Vec<&Expression> = Vec::new();
        let mut covered = self.enum_coverage(&subject_type);
        let mut result: Option<Option<Type>> = expected.map(|expected| Some(expected.clone()));
        for arm in cases {
            let pushed =
                self.match_pattern(&arm.pattern, &subject_type, &mut seen, covered.as_mut())?;
            let ty = match (&arm.body, expected) {
                (MatchBody::Expression(expression), Some(expected)) => {
                    Some(self.require(expression, expected)?)
                }
                _ => self.match_body_type(&arm.body)?,
            };
            for _ in 0..pushed {
                self.overrides.pop();
            }
            if let Some(previous) = &result
                && previous != &ty
            {
                return Err(self.error(
                    arm.span,
                    expected_instead(
                        &self.optional_type_name(previous),
                        &self.optional_type_name(&ty),
                    ),
                ));
            }
            result = Some(ty);
        }
        if let Some(default) = default {
            let ty = match (default, expected) {
                (MatchBody::Expression(expression), Some(expected)) => {
                    Some(self.require(expression, expected)?)
                }
                _ => self.match_body_type(default)?,
            };
            if let Some(previous) = &result
                && previous != &ty
            {
                return Err(self.error(
                    subject.span(),
                    expected_instead(
                        &self.optional_type_name(previous),
                        &self.optional_type_name(&ty),
                    ),
                ));
            }
            result = Some(ty);
        }
        if !is_exhaustive(default, &subject_type, cases, covered.as_deref()) {
            return Err(self.error(
                span,
                "a match that produces a value must cover every case; add a `_` arm",
            ));
        }
        Ok(result.flatten())
    }

    /// The coverage tracker for an enum subject: one flag per variant, or
    /// `None` for a non-enum.
    pub(super) fn enum_coverage(&self, subject_type: &Type) -> Option<Vec<bool>> {
        let Type::Named(id, _) = subject_type else {
            return None;
        };
        if !self.program.type_decl(*id).group().has_variants() {
            return None;
        }
        let TypeKind::Enum(variants) = &self.program.type_decl(*id).kind else {
            unreachable!("a group with variants is an enum");
        };
        Some(vec![false; variants.len()])
    }

    /// Validate one match arm's pattern against the subject type, tracking
    /// duplicate value patterns and covered enum variants. Returns how many
    /// binding overrides it pushed, which the caller pops after the arm body.
    pub(super) fn match_pattern<'e>(
        &mut self,
        pattern: &'e Pattern,
        subject_type: &Type,
        seen: &mut Vec<&'e Expression>,
        covered: Option<&mut Vec<bool>>,
    ) -> Result<usize, Diagnostic> {
        match pattern {
            Pattern::Value(expression) => {
                if !is_atom(expression) {
                    return Err(self.error(
                        expression.span(),
                        "a match arm is a literal, a name, or a field path",
                    ));
                }
                // Two arms are duplicates when they are the same atom: equal
                // literal value, the same declaration, or the same field path.
                if seen.iter().any(|other| atoms_equal(other, expression)) {
                    return Err(
                        self.error(expression.span(), duplicate_pattern_message(expression))
                    );
                }
                self.require(expression, subject_type)?;
                seen.push(expression);
                Ok(0)
            }
            Pattern::Variant {
                ty,
                index,
                bindings,
                span,
            } => {
                let Type::Named(id, subject_arguments) = subject_type else {
                    return Err(self.error(
                        *span,
                        expected_instead("an enum", &self.type_name(subject_type)),
                    ));
                };
                if id != ty {
                    return Err(self.error(
                        *span,
                        expected_instead(
                            &self.type_name(subject_type),
                            &self.type_name(&Type::Named(*ty, Vec::new())),
                        ),
                    ));
                }
                let Some(covered) = covered else {
                    return Err(self.error(*span, "a variant pattern needs an enum subject"));
                };
                if covered[*index] {
                    return Err(self.error(*span, "duplicate match pattern"));
                }
                covered[*index] = true;
                let (variant_name, parameters, public, declared) = {
                    let declaration = self.program.type_decl(*ty);
                    let TypeKind::Enum(variants) = &declaration.kind else {
                        unreachable!("a variant pattern names an enum");
                    };
                    let variant = &variants[*index];
                    (
                        variant.name.clone(),
                        declaration.parameters.clone(),
                        variant.public,
                        variant.payload.clone(),
                    )
                };
                self.check_variant(*ty, &variant_name, public, *span)?;
                for (binding, field_type) in bindings.iter().zip(&declared) {
                    if let Some(binding) = binding {
                        let ty = substitute(
                            field_type,
                            &self.program.types,
                            &parameters,
                            Binding::Complete(subject_arguments),
                        );
                        self.overrides.push((*binding, ty));
                    }
                }
                Ok(bindings.iter().filter(|binding| binding.is_some()).count())
            }
        }
    }

    /// The type of a match arm's body: an expression's type, or no type for a
    /// block (a block produces no value).
    fn match_body_type(&mut self, body: &MatchBody) -> Result<Option<Type>, Diagnostic> {
        match body {
            MatchBody::Expression(expression) => self.expression(expression, None),
            MatchBody::Block(statements) => {
                self.statements(statements)?;
                Ok(None)
            }
        }
    }
}
