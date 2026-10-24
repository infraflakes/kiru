//! Match rules: match legality, coverage, and typing.
//!
//! A match arm is an atom — a text or map literal, a name, or a field path —
//! never a call, a concatenation, or an interpolation. That keeps matching free
//! of side effects and makes duplicate detection a comparison of atoms. A match
//! that produces a value must cover every case; a statement match need not.

use crate::compiler::Diagnostic;
use crate::compiler::diagnostic::expected_instead;
use crate::compiler::model::{
    Binding, Expression, Finite, MatchArm, MatchBody, Pattern, Site, Type, TypeKind, finite,
    substitute,
};
use crate::syntax::Span;

use super::statement_check::Checker;

/// The coverage of one finite subject: the values its type declares it can
/// take, and whether an arm has covered each. One tracker serves a boolean's
/// two values and an enum's variants, so a single pass decides exhaustiveness.
pub(super) struct Coverage {
    finite: Finite,
    covered: Vec<bool>,
}

impl Coverage {
    /// The tracker for a finite subject, or `None` when the subject is open.
    fn new(finite: Finite, value_count: usize) -> Option<Self> {
        match finite {
            Finite::Open => None,
            Finite::Booleans => Some(Self {
                finite,
                covered: vec![false; 2],
            }),
            Finite::Variants => Some(Self {
                finite,
                covered: vec![false; value_count],
            }),
        }
    }

    /// Mark the value a pattern covers. Returns whether that value was already
    /// covered, so a repeated finite value is a duplicate arm; a pattern that
    /// names no finite value, or one this tracker does not enumerate, covers
    /// nothing.
    fn mark(&mut self, pattern: &Pattern) -> bool {
        let index = match (self.finite, pattern) {
            (Finite::Variants, Pattern::Variant { index, .. }) => *index,
            (Finite::Booleans, Pattern::Value(Expression::Bool { value, .. })) => {
                usize::from(*value)
            }
            _ => return false,
        };
        let Some(covered) = self.covered.get_mut(index) else {
            return false;
        };
        let already = *covered;
        *covered = true;
        already
    }

    /// Whether every value the subject can take has an arm.
    fn complete(&self) -> bool {
        self.covered.iter().all(|covered| *covered)
    }
}

/// Whether a match takes an arm for every value: it has a `_` arm, or its
/// subject declares a finite set and every one of its values is covered.
pub(super) fn is_exhaustive(default: &Option<MatchBody>, coverage: Option<&Coverage>) -> bool {
    default.is_some() || coverage.is_some_and(Coverage::complete)
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
        _ => false,
    }
}

/// The diagnostic for a repeated match arm; a literal text is quoted.
fn duplicate_pattern_message(pattern: &Expression) -> String {
    match pattern {
        Expression::Text { value, .. } => format!("duplicate match pattern `{value}`"),
        Expression::Bool { value, .. } => format!("duplicate match pattern `{value}`"),
        Expression::Int { value, .. } => format!("duplicate match pattern `{value}`"),
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
    /// always yields a value. The coverage the flow pass reads is recorded in
    /// `Facts`, not written onto the node.
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
        let mut coverage = self.coverage(&subject_type);
        let mut result: Option<Option<Type>> = expected.map(|expected| Some(expected.clone()));
        for arm in cases {
            self.match_pattern(&arm.pattern, &subject_type, &mut seen, coverage.as_mut())?;
            let ty = match (&arm.body, expected) {
                (MatchBody::Expression(expression), Some(expected)) => {
                    Some(self.require(expression, expected)?)
                }
                (MatchBody::Expression(expression), None) => self.expression(expression, None)?,
                (MatchBody::Block(statements), _) => {
                    self.statements(statements)?;
                    None
                }
            };
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
                (MatchBody::Expression(expression), None) => self.expression(expression, None)?,
                (MatchBody::Block(statements), _) => {
                    self.statements(statements)?;
                    None
                }
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
        let complete = is_exhaustive(default, coverage.as_ref());
        self.facts
            .set_match_exhaustive(Site::new(self.owner, span), complete);
        if !complete {
            return Err(self.error(
                span,
                "a match that produces a value must cover every case; add a `_` arm",
            ));
        }
        Ok(result.flatten())
    }

    /// The coverage tracker for a subject: every value its type declares it can
    /// take, or `None` when it declares no finite set.
    pub(super) fn coverage(&self, subject_type: &Type) -> Option<Coverage> {
        let finite = finite(subject_type, self.model.types());
        let value_count = match finite {
            Finite::Variants => {
                let Type::Named(id, _) = subject_type else {
                    unreachable!("a finite variant subject is a named type");
                };
                let TypeKind::Enum(variants) = &self.model.type_decl(*id).kind else {
                    unreachable!("a finite variant subject is an enum");
                };
                variants.len()
            }
            Finite::Booleans | Finite::Open => 0,
        };
        Coverage::new(finite, value_count)
    }

    /// Validate one match arm's pattern against the subject type, tracking
    /// duplicate value patterns and covered enum variants, and record each
    /// binding's substituted type in `Facts`. A binding is created once,
    /// belongs to one pattern, and has one instantiation, so its arm's fact is
    /// the only place its instantiated type is read.
    pub(super) fn match_pattern<'e>(
        &mut self,
        pattern: &'e Pattern,
        subject_type: &Type,
        seen: &mut Vec<&'e Expression>,
        mut coverage: Option<&mut Coverage>,
    ) -> Result<(), Diagnostic> {
        match pattern {
            Pattern::Value(_) => {
                // Mark coverage before typing the pattern's expression.
                if let Some(coverage) = coverage.as_mut() {
                    coverage.mark(pattern);
                }
                let Pattern::Value(expression) = pattern else {
                    unreachable!("the pattern is a value pattern");
                };
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
                Ok(())
            }
            Pattern::Variant { .. } => {
                let tracker = coverage.is_some();
                let already = coverage
                    .as_mut()
                    .is_some_and(|coverage| coverage.mark(pattern));
                let Pattern::Variant {
                    ty,
                    index,
                    bindings,
                    span,
                } = pattern
                else {
                    unreachable!("the pattern is a variant pattern");
                };
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
                if !tracker {
                    return Err(self.error(*span, "a variant pattern needs an enum subject"));
                }
                if already {
                    return Err(self.error(*span, "duplicate match pattern"));
                }
                let (variant_name, parameters, public, declared) = {
                    let declaration = self.model.type_decl(*ty);
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
                self.check_member(*ty, &variant_name, public, *span)?;
                for (binding, field_type) in bindings.iter().zip(&declared) {
                    if let Some(binding) = binding {
                        let ty = substitute(
                            field_type,
                            self.model.types(),
                            &parameters,
                            Binding::Complete(subject_arguments),
                        );
                        self.facts.set_pattern_type(*binding, ty);
                    }
                }
                Ok(())
            }
        }
    }
}
