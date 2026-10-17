//! Typing expressions in one declaration body.
//!
//! One relation types every expression: `expression(expression, expected)`
//! derives an expression's type and, when the position asks for one, checks it
//! against that type. A shape literal takes its type from the expected type;
//! `fits` is the leaf equality. A function's return type is declared, so a
//! return is checked against it rather than accumulated, and a call that
//! produces no value may only stand as a statement.

use crate::compiler::{
    Diagnostic, duplicate_field, expected_instead, native_used_as_value, value_called,
};
use crate::model::{
    DeclarationId, Expression, Field, MatchArm, MatchBody, Pattern, StringPart, TypeId, TypeKind,
    first_duplicate, rec_field_decl,
};
use crate::model::{Type, fits, has_parameter, substitute, substitute_optional, unify};
use crate::syntax::{BinaryOp, Span, UnaryOp};

use super::body_check::Checker;
use super::patterns::{atoms_equal, covers_booleans, duplicate_pattern_message, is_atom};

/// Whether an expression is a record literal, which takes its type from the
/// position it is checked against rather than from its own form.
fn is_map_literal(expression: &Expression) -> bool {
    matches!(expression, Expression::Map { .. })
}

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

impl<'a> Checker<'a> {
    /// The type of a function value: the declaration's own type, which is a
    /// function type. A generic function has no one type, so it cannot be a
    /// value.
    fn function_value(&self, declaration: DeclarationId, span: Span) -> Result<Type, Diagnostic> {
        let function = self
            .program
            .declaration(declaration)
            .function()
            .expect("a function value names a function");
        if !function.type_parameters.is_empty() {
            return Err(self.error(
                span,
                format!(
                    "`{}` is generic; give it a type annotation to use it as a value",
                    self.program.display(declaration)
                ),
            ));
        }
        Ok(self.program.declaration(declaration).ty.clone())
    }
    /// Reject a record literal that repeats a field name. A record literal is a
    /// declared shape, so a repeated field is always a mistake.
    fn reject_duplicate_fields(&self, fields: &[Field]) -> Result<(), Diagnostic> {
        if let Some(index) = first_duplicate(fields.iter().map(|field| field.name.as_str())) {
            let field = &fields[index];
            return Err(self.error(field.name_span, duplicate_field(&field.name)));
        }
        Ok(())
    }

    /// The one type relation. Type `expression` in a position that asks for
    /// `expected`, when the position names one. An expression with an intrinsic
    /// type — a literal, a name, a call, a field, an operator — yields that type
    /// and is then compared with the expected one by `fits`. A shape literal has
    /// no type of its own: a map, a Vec, an enum variant, and a match take
    /// their type from the expected type; a Vec or a match is inferred from its
    /// parts only where no type is asked for. Every value position names its
    /// requirement as a type, so a shape is checked against its type wherever it
    /// appears.
    pub(super) fn expression(
        &mut self,
        expression: &Expression,
        expected: Option<&Type>,
    ) -> Result<Option<Type>, Diagnostic> {
        let actual = match expression {
            Expression::Text { .. } => Ok(Some(Type::Text)),
            Expression::Bool { .. } => Ok(Some(Type::Bool)),
            Expression::Int { .. } => Ok(Some(Type::Int)),
            // A `{ … }` literal is a declared shape; the expected type must be
            // a declared rec, and with no expected type it has none.
            Expression::Map { fields, span } => match expected {
                Some(Type::Named(id, arguments)) => {
                    self.map_against(*id, arguments, fields, *span)?;
                    Ok(Some(Type::Named(*id, arguments.clone())))
                }
                Some(other) => Err(self.error(
                    *span,
                    expected_instead(&self.type_name(other), "a `{ … }` literal"),
                )),
                None => Err(self.error(
                    *span,
                    "cannot infer the type of this `{ … }` literal; add a type annotation",
                )),
            },
            Expression::Variant {
                ty,
                index,
                payload,
                span,
            } => {
                // A variant named by the expected type is checked against it;
                // otherwise its own declaration supplies the type, and a
                // parameterized one has no type to supply.
                if let Some(Type::Named(id, arguments)) = expected
                    && id == ty
                {
                    self.variant_against(*ty, *index, arguments, payload, *span)?;
                    Ok(Some(Type::Named(*id, arguments.clone())))
                } else {
                    if !self.program.type_decl(*ty).parameters.is_empty() {
                        return Err(self.error(
                            *span,
                            format!(
                                "cannot infer the type arguments of `{}`; add a type annotation",
                                self.type_name(&Type::Named(*ty, Vec::new()))
                            ),
                        ));
                    }
                    self.variant_against(*ty, *index, &[], payload, *span)?;
                    Ok(Some(Type::Named(*ty, Vec::new())))
                }
            }
            // A Vec literal takes its element type from the expected type, or
            // from its first element when none is asked for.
            Expression::Vec { elements, .. } => {
                if let Some(Type::Vec(element)) = expected {
                    for item in elements {
                        self.require(item, element)?;
                    }
                    Ok(Some(Type::Vec(element.clone())))
                } else {
                    let mut element_type: Option<Type> = None;
                    for element in elements {
                        let Some(ty) = self.expression(element, None)? else {
                            return Err(
                                self.error(element.span(), "a Vec element must have a value")
                            );
                        };
                        if let Some(previous) = &element_type
                            && previous != &ty
                        {
                            return Err(self.error(
                                element.span(),
                                expected_instead(&self.type_name(previous), &self.type_name(&ty)),
                            ));
                        }
                        element_type = Some(ty);
                    }
                    Ok(Some(Type::Vec(Box::new(
                        element_type.unwrap_or(Type::Text),
                    ))))
                }
            }
            Expression::Reference { declaration, span } => {
                self.reference_value(*declaration, *span)
            }
            // A function literal is a value whose type is a function type. Its
            // body is proved here, where it is written, so a captured loop
            // variable or pattern binding keeps the type the enclosing walk
            // knows.
            Expression::Function(function) => {
                self.check_function(function, "<closure>")?;
                Ok(Some(function.ty.clone()))
            }
            Expression::Call {
                callee,
                callee_span,
                arguments,
                ..
            } => self.call(callee, *callee_span, arguments, expected),
            Expression::Interpolated { parts, .. } => {
                for part in parts {
                    if let StringPart::Expression(expression) = part {
                        self.displayable(expression)?;
                    }
                }
                Ok(Some(Type::Text))
            }
            // A match takes its type from the expected type, or from the common
            // type of its arms when none is asked for. It has no type when its
            // arms are blocks.
            Expression::Match {
                subject,
                cases,
                default,
                span,
            } => self.match_expression(*span, subject, cases, default, expected),
            Expression::Field {
                target,
                name,
                name_span,
                ..
            } => {
                let Some(target_type) = self.expression(target, None)? else {
                    return Err(self.error(target.span(), expected_instead("a rec", "no value")));
                };
                match target_type {
                    Type::Named(id, arguments) => {
                        let declaration = self.program.type_decl(id);
                        match &declaration.kind {
                            TypeKind::Rec(_) => {
                                let Some(field) = rec_field_decl(&self.program.types, id, name)
                                else {
                                    let message =
                                        format!("`{}` has no field `{name}`", declaration.name);
                                    return Err(self.error(*name_span, message));
                                };
                                self.check_field(id, name, field.public, *name_span)?;
                                let ty = substitute(&field.ty, &declaration.parameters, &arguments);
                                Ok(Some(ty))
                            }
                            TypeKind::Enum(_) => Err(self.error(
                                *name_span,
                                format!("`{}` is an enum; match it", declaration.name),
                            )),
                        }
                    }
                    other => Err(self.error(
                        target.span(),
                        expected_instead("a rec", &self.type_name(&other)),
                    )),
                }
            }
            Expression::Index { target, index, .. } => {
                let Some(target_type) = self.expression(target, None)? else {
                    return Err(self.error(target.span(), expected_instead("Vec", "no value")));
                };
                let element = match target_type {
                    Type::Vec(element) => *element,
                    other => {
                        return Err(self.error(
                            target.span(),
                            expected_instead("Vec", &self.type_name(&other)),
                        ));
                    }
                };
                self.require(index, &Type::Int)?;
                Ok(Some(element))
            }
            Expression::Binary {
                op, left, right, ..
            } => self.binary(*op, left, right),
            Expression::Unary { op, operand, .. } => self.unary(*op, operand),
        }?;
        match (&actual, expected) {
            (Some(actual), Some(required)) if !fits(actual, required) => {
                return Err(self.error(
                    expression.span(),
                    expected_instead(&self.type_name(required), &self.type_name(actual)),
                ));
            }
            // An expression that produces no value cannot stand where a value
            // is asked for: there is no type that names it.
            (None, Some(required)) => {
                return Err(self.error(
                    expression.span(),
                    expected_instead(&self.type_name(required), "no value"),
                ));
            }
            _ => {}
        }
        Ok(actual)
    }

    /// The type of a binary operator: integer arithmetic, integer ordering,
    /// equality over any value, and boolean logic.
    fn binary(
        &mut self,
        op: BinaryOp,
        left: &Expression,
        right: &Expression,
    ) -> Result<Option<Type>, Diagnostic> {
        match op {
            BinaryOp::Add
            | BinaryOp::Subtract
            | BinaryOp::Multiply
            | BinaryOp::Divide
            | BinaryOp::Remainder => {
                self.require(left, &Type::Int)?;
                self.require(right, &Type::Int)?;
                Ok(Some(Type::Int))
            }
            BinaryOp::Less | BinaryOp::LessEqual | BinaryOp::Greater | BinaryOp::GreaterEqual => {
                let Some(left_type) = self.expression(left, None)? else {
                    return Err(self.error(left.span(), expected_instead("int or txt", "no value")));
                };
                self.require(right, &left_type)?;
                match left_type {
                    Type::Int | Type::Text => Ok(Some(Type::Bool)),
                    other => Err(self.error(
                        left.span(),
                        format!(
                            "`{}` cannot be ordered; compare `int` or `txt`",
                            self.type_name(&other)
                        ),
                    )),
                }
            }
            BinaryOp::Equal | BinaryOp::NotEqual => {
                // A record literal takes its type from the other operand, so
                // equality reads the same whichever side the declared value is.
                let (typed, checked) = if is_map_literal(left) && !is_map_literal(right) {
                    (right, left)
                } else {
                    (left, right)
                };
                let Some(ty) = self.expression(typed, None)? else {
                    return Err(self.error(
                        typed.span(),
                        "`==` cannot compare a call that returns nothing",
                    ));
                };
                self.require(checked, &ty)?;
                Ok(Some(Type::Bool))
            }
            BinaryOp::And | BinaryOp::Or => {
                self.require(left, &Type::Bool)?;
                self.require(right, &Type::Bool)?;
                Ok(Some(Type::Bool))
            }
        }
    }

    /// The type of a prefix operator: integer negation or boolean not.
    fn unary(&mut self, op: UnaryOp, operand: &Expression) -> Result<Option<Type>, Diagnostic> {
        match op {
            UnaryOp::Negate => {
                self.require(operand, &Type::Int)?;
                Ok(Some(Type::Int))
            }
            UnaryOp::Not => {
                self.require(operand, &Type::Bool)?;
                Ok(Some(Type::Bool))
            }
        }
    }

    /// Require a value that `@(…)` can insert: text, an integer, or a boolean.
    /// A map, a Vec, and a no-value call have no display form.
    fn displayable(&mut self, expression: &Expression) -> Result<(), Diagnostic> {
        let Some(ty) = self.expression(expression, None)? else {
            return Err(self.error(
                expression.span(),
                "`@(…)` inserts text, an integer, or a boolean, found no value",
            ));
        };
        if matches!(ty, Type::Text | Type::Int | Type::Bool) {
            return Ok(());
        }
        Err(self.error(
            expression.span(),
            format!(
                "`@(…)` inserts text, an integer, or a boolean, found {}",
                self.type_name(&ty)
            ),
        ))
    }

    /// The type of a name used as a value: a function name has a function type,
    /// and a binding or module value has its declared type. A function value is
    /// just a value whose type is a function type.
    fn reference_value(
        &mut self,
        declaration: DeclarationId,
        span: Span,
    ) -> Result<Option<Type>, Diagnostic> {
        if self.program.declaration(declaration).is_native() {
            return Err(self.error(
                span,
                native_used_as_value(&self.program.display(declaration)),
            ));
        }
        if self.program.declaration(declaration).function().is_some() {
            return Ok(Some(self.function_value(declaration, span)?));
        }
        Ok(Some(self.kind_of(declaration)))
    }

    /// The type of an application. The callee is an expression and its type
    /// decides: a value of a function type is called through its value, and a
    /// reference to a declared callable names the function its declaration
    /// holds. The one distinction is generic instantiation and diagnostics: a
    /// declared generic function's type parameters are bound by the call and by
    /// the expected type, and its name labels an argument error. A native has no
    /// value form and is the one callee dispatched by index.
    fn call(
        &mut self,
        callee: &Expression,
        callee_span: Span,
        arguments: &[Expression],
        expected: Option<&Type>,
    ) -> Result<Option<Type>, Diagnostic> {
        if let Expression::Reference { declaration, .. } = callee
            && self.program.declaration(*declaration).is_callable()
        {
            return self.direct_call(*declaration, callee_span, arguments, expected);
        }
        self.call_value(callee, arguments)
    }

    /// A call that names a declared callable: its declared type supplies the
    /// parameter and return types and, for a generic function, the type
    /// parameters the call instantiates. The expected type at the call site
    /// binds a type parameter the arguments cannot, so `empty()` may infer its
    /// arguments from `let m: Map<txt, txt> = …`. A callable with no return
    /// type yields no type, so its calls produce no value.
    fn direct_call(
        &mut self,
        callee: DeclarationId,
        callee_span: Span,
        arguments: &[Expression],
        expected: Option<&Type>,
    ) -> Result<Option<Type>, Diagnostic> {
        let (returns, type_parameters, declared) = {
            let node = self.program.declaration(callee);
            let returns = node.return_type();
            let type_parameters = node
                .function()
                .map(|function| function.type_parameters.clone())
                .unwrap_or_default();
            let declared = match &node.ty {
                Type::Function { parameters, .. } => parameters.clone(),
                _ => unreachable!("a callable has a function type"),
            };
            (returns, type_parameters, declared)
        };
        self.arity(callee, callee_span, arguments, declared.len())?;
        let mut solution: Vec<Option<Type>> = vec![None; type_parameters.len()];
        // The expected type binds the callee's type parameters: the call's
        // result is the type the position asks for, so the declared return type
        // is matched against it. The bindings are held aside, not committed, so
        // an argument that settles a parameter keeps its own binding — a
        // conflicting argument is still the error that names the argument. A
        // shape that does not match binds nothing, and the final check reports
        // it.
        let mut expected_bindings: Vec<Option<Type>> = vec![None; type_parameters.len()];
        if let (Some(expected), Some(returns)) = (expected, &returns)
            && !unify(returns, expected, &type_parameters, &mut expected_bindings)
        {
            expected_bindings = vec![None; type_parameters.len()];
        }
        // Infer each type parameter the expected type left open from its
        // argument's type. A compound parameter the bindings already settle is
        // passed to the argument as its expected type, so a literal argument
        // takes its shape — that is how `from([{ key = …, value = … }])` reads.
        // A bare parameter is typed on its own, so a conflicting argument still
        // reports against the argument.
        for (argument, parameter) in arguments.iter().zip(&declared) {
            if has_parameter(parameter) {
                let mut hint_solution = solution.clone();
                for (slot, expected) in hint_solution.iter_mut().zip(&expected_bindings) {
                    if slot.is_none() {
                        *slot = expected.clone();
                    }
                }
                let hint_bound = substitute_optional(parameter, &type_parameters, &hint_solution);
                let hint = (!has_parameter(&hint_bound)
                    && !matches!(parameter, Type::Parameter(_)))
                .then_some(&hint_bound);
                let Some(actual) = self.expression(argument, hint)? else {
                    return Err(self.error(
                        argument.span(),
                        format!(
                            "`{}` cannot take an argument that produces no value",
                            self.program.display(callee)
                        ),
                    ));
                };
                if !unify(parameter, &actual, &type_parameters, &mut solution) {
                    return Err(self.error(
                        argument.span(),
                        format!(
                            "`{}` cannot take an argument of type {}",
                            self.program.display(callee),
                            self.type_name(&actual)
                        ),
                    ));
                }
            }
        }
        // Fill every parameter the arguments left open from the expected type.
        for (slot, expected) in solution.iter_mut().zip(&expected_bindings) {
            if slot.is_none() {
                *slot = expected.clone();
            }
        }
        let mut inferred = Vec::with_capacity(type_parameters.len());
        for (index, bound) in solution.into_iter().enumerate() {
            let Some(bound) = bound else {
                return Err(self.error(
                    callee_span,
                    format!(
                        "cannot infer the type parameter `{}` of `{}`; add a type annotation",
                        type_parameters[index],
                        self.program.display(callee)
                    ),
                ));
            };
            inferred.push(bound);
        }
        for (argument, parameter) in arguments.iter().zip(&declared) {
            let required = substitute(parameter, &type_parameters, &inferred);
            self.require(argument, &required)?;
        }
        Ok(returns.map(|returns| substitute(&returns, &type_parameters, &inferred)))
    }

    /// The type of an indirect call: the callee must be a function value. A
    /// function value with no return type yields no type.
    fn call_value(
        &mut self,
        callee: &Expression,
        arguments: &[Expression],
    ) -> Result<Option<Type>, Diagnostic> {
        let callee_type = self.expression(callee, None)?;
        let Some(callee_type) = callee_type else {
            if let Expression::Reference { declaration, .. } = callee {
                return Err(self.error(
                    callee.span(),
                    value_called(&self.program.display(*declaration)),
                ));
            }
            return Err(self.error(callee.span(), "a call's callee must produce a value"));
        };
        let Type::Function {
            parameters,
            returns,
        } = callee_type
        else {
            if let Expression::Reference { declaration, .. } = callee {
                return Err(self.error(
                    callee.span(),
                    value_called(&self.program.display(*declaration)),
                ));
            }
            return Err(self.error(
                callee.span(),
                expected_instead("a function", &self.type_name(&callee_type)),
            ));
        };
        if arguments.len() != parameters.len() {
            return Err(self.error(
                callee.span(),
                format!(
                    "a function value takes {}, found {}",
                    crate::compiler::diagnostics::count(parameters.len(), "argument"),
                    arguments.len()
                ),
            ));
        }
        for (argument, required) in arguments.iter().zip(&parameters) {
            self.require(argument, required)?;
        }
        Ok(returns.map(|returns| *returns))
    }

    fn arity(
        &self,
        callee: DeclarationId,
        span: Span,
        arguments: &[Expression],
        expected: usize,
    ) -> Result<(), Diagnostic> {
        if arguments.len() != expected {
            return Err(self.error(
                span,
                format!(
                    "`{}` takes {}, found {}",
                    self.program.display(callee),
                    crate::compiler::diagnostics::count(expected, "argument"),
                    arguments.len()
                ),
            ));
        }
        Ok(())
    }

    /// Require a value of a type, returning that type. The value-position entry
    /// to the one relation: `expression` with the position's declared type. An
    /// expression that produces no value is rejected here.
    pub(in crate::compiler::validate_program) fn require(
        &mut self,
        expression: &Expression,
        required: &Type,
    ) -> Result<Type, Diagnostic> {
        self.expression(expression, Some(required))?.ok_or_else(|| {
            self.error(
                expression.span(),
                expected_instead(&self.type_name(required), "no value"),
            )
        })
    }

    /// Check a record literal against a declared type: every declared field is
    /// present with a fitting value, and no other field appears. The declared
    /// field types are substituted with the applied type arguments.
    fn map_against(
        &mut self,
        id: TypeId,
        arguments: &[Type],
        fields: &[Field],
        span: Span,
    ) -> Result<(), Diagnostic> {
        let declaration = self.program.type_decl(id);
        let TypeKind::Rec(declared_fields) = &declaration.kind else {
            return Err(self.error(span, format!("`{}` has no fields", declaration.name)));
        };
        let name = declaration.name.clone();
        let parameters = declaration.parameters.clone();
        let declared: Vec<(String, bool, Type)> = declared_fields
            .iter()
            .map(|field| {
                (
                    field.name.clone(),
                    field.public,
                    substitute(&field.ty, &parameters, arguments),
                )
            })
            .collect();

        self.reject_duplicate_fields(fields)?;
        for (field_name, public, field_ty) in &declared {
            let Some(field) = fields.iter().find(|field| &field.name == field_name) else {
                return Err(self.error(span, format!("`{name}` is missing field `{field_name}`")));
            };
            self.check_field(id, field_name, *public, field.name_span)?;
            self.require(&field.value, field_ty)?;
        }
        for field in fields {
            if !declared.iter().any(|(other, _, _)| other == &field.name) {
                return Err(self.error(
                    field.name_span,
                    format!("`{name}` has no field `{}`", field.name),
                ));
            }
        }
        Ok(())
    }

    /// Check an enum construction against a declared type: the variant exists,
    /// the payload count matches, and every payload fits the substituted type.
    fn variant_against(
        &mut self,
        id: TypeId,
        index: usize,
        arguments: &[Type],
        payload: &[Expression],
        span: Span,
    ) -> Result<(), Diagnostic> {
        let (name, variant_name, public, parameters, declared) = {
            let declaration = self.program.type_decl(id);
            let TypeKind::Enum(variants) = &declaration.kind else {
                return Err(self.error(span, format!("`{}` is not an enum", declaration.name)));
            };
            let Some(variant) = variants.get(index) else {
                return Err(self.error(span, "unknown enum variant"));
            };
            (
                declaration.name.clone(),
                variant.name.clone(),
                variant.public,
                declaration.parameters.clone(),
                variant.payload.clone(),
            )
        };
        self.check_variant(id, &variant_name, public, span)?;
        if payload.len() != declared.len() {
            return Err(self.error(
                span,
                format!(
                    "`{name}.{variant_name}` takes {}, found {}",
                    crate::compiler::diagnostics::count(declared.len(), "argument"),
                    payload.len()
                ),
            ));
        }
        for (argument, required) in payload.iter().zip(&declared) {
            let required = substitute(required, &parameters, arguments);
            self.require(argument, &required)?;
        }
        Ok(())
    }
    /// The name of an optional type for a diagnostic: `no value` when the
    /// expression produced none.
    fn optional_type_name(&self, ty: &Option<Type>) -> String {
        match ty {
            Some(ty) => self.type_name(ty),
            None => "no value".to_owned(),
        }
    }

    /// The type a match compares. Its subject must produce a value: text, a
    /// map, or a Vec, never a call that produces none.
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
    fn match_expression(
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
        match &self.program.type_decl(*id).kind {
            TypeKind::Enum(variants) => Some(vec![false; variants.len()]),
            TypeKind::Rec(_) => None,
        }
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
                        let ty = substitute(field_type, &parameters, subject_arguments);
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
