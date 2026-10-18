//! Typing expressions in one declaration body.
//!
//! One relation types every expression: `expression(expression, expected)`
//! derives an expression's type and, when the position asks for one, checks it
//! against that type. A shape literal takes its type from the expected type;
//! `fits` is the leaf equality. A function's return type is declared, so a
//! return is checked against it rather than accumulated, and a call that
//! produces no value may only stand as a statement.

use crate::compiler::model::{
    Binding, DeclarationId, Expression, Field, Group, RuntimeShape, Shape, StringPart, Type,
    TypeId, TypeKind, first_duplicate, fits, has_parameter, rec_field, substitute, unify,
};
use crate::compiler::{
    Diagnostic, duplicate_field, expected_instead, native_used_as_value, value_called,
};
use crate::syntax::{BinaryOp, Span, UnaryOp};

use super::statement_check::Checker;

/// Whether an expression is a record literal, which takes its type from the
/// position it is checked against rather than from its own form.
fn is_map_literal(expression: &Expression) -> bool {
    matches!(expression, Expression::Map { .. })
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
    /// no type of its own: a map, an enum variant, and a match take
    /// their type from the expected type; a match is inferred from its
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
            // a declared rec that takes a literal, and with no expected type it
            // has none. The runtime's `Vec` is a declaration with no fields,
            // but its representation is the runtime's list, so it has no
            // literal form.
            Expression::Map { fields, span } => match expected {
                Some(Type::Named(id, arguments)) if self.program.vec_type != Some(*id) => {
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
                        if declaration.group().has_variants() {
                            return Err(self.error(
                                *name_span,
                                format!("`{}` is an enum; match it", declaration.name),
                            ));
                        }
                        let Some(field) = rec_field(&self.program.types, id, name) else {
                            let message = format!("`{}` has no field `{name}`", declaration.name);
                            return Err(self.error(*name_span, message));
                        };
                        self.check_field(id, name, field.public, *name_span)?;
                        let ty = substitute(
                            &field.ty,
                            &self.program.types,
                            &declaration.parameters,
                            Binding::Complete(&arguments),
                        );
                        Ok(Some(ty))
                    }
                    other => Err(self.error(
                        target.span(),
                        expected_instead("a rec", &self.type_name(&other)),
                    )),
                }
            }
            Expression::Binary {
                op, left, right, ..
            } => self.binary(*op, left, right),
            Expression::Unary { op, operand, .. } => self.unary(*op, operand),
        }?;
        match (&actual, expected) {
            (Some(actual), Some(required)) if !fits(actual, required, &self.program.types) => {
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
        if ty.group(&self.program.types).runtime_shape() == RuntimeShape::Scalar {
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
            let returns = node.return_type(&self.program.types);
            let type_parameters = node.type_parameters(&self.program.types);
            let declared = match node.ty.group(&self.program.types) {
                Group::Function => {
                    let Type::Function { parameters, .. } = &node.ty else {
                        unreachable!("a function group is a function type");
                    };
                    parameters.clone()
                }
                Group::Scalar | Group::Rec | Group::Enum | Group::Parameter => {
                    unreachable!("a callable has a function type")
                }
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
            && !unify(
                returns,
                expected,
                &self.program.types,
                &type_parameters,
                &mut expected_bindings,
            )
        {
            expected_bindings = vec![None; type_parameters.len()];
        }
        // Infer each type parameter the expected type left open from its
        // argument's type. A parameter the bindings already settle is passed
        // to the argument as its expected type, so a literal argument takes
        // its shape: that is how `xs.push({ key = …, value = … })` reads once
        // the receiver bound the element. A bare type parameter reaches only
        // a `{ … }` literal, whose type comes from the position alone; an
        // argument with a type of its own is typed on its own, so a
        // conflicting one still reports against the argument.
        for (argument, parameter) in arguments.iter().zip(&declared) {
            if has_parameter(parameter, &self.program.types) {
                let mut hint_solution = solution.clone();
                for (slot, expected) in hint_solution.iter_mut().zip(&expected_bindings) {
                    if slot.is_none() {
                        *slot = expected.clone();
                    }
                }
                let hint_bound = substitute(
                    parameter,
                    &self.program.types,
                    &type_parameters,
                    Binding::Partial(&hint_solution),
                );
                let hint = (!has_parameter(&hint_bound, &self.program.types)
                    && (parameter.group(&self.program.types) != Group::Parameter
                        || is_map_literal(argument)))
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
                if !unify(
                    parameter,
                    &actual,
                    &self.program.types,
                    &type_parameters,
                    &mut solution,
                ) {
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
            let required = substitute(
                parameter,
                &self.program.types,
                &type_parameters,
                Binding::Complete(&inferred),
            );
            self.require(argument, &required)?;
        }
        Ok(returns.map(|returns| {
            substitute(
                &returns,
                &self.program.types,
                &type_parameters,
                Binding::Complete(&inferred),
            )
        }))
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
        if callee_type.group(&self.program.types) != Group::Function {
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
        }
        let Type::Function {
            parameters,
            returns,
        } = callee_type
        else {
            unreachable!("a function group is a function type");
        };
        if arguments.len() != parameters.len() {
            return Err(self.error(
                callee.span(),
                format!(
                    "a function value takes {}, found {}",
                    crate::compiler::messages::count(parameters.len(), "argument"),
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
                    crate::compiler::messages::count(expected, "argument"),
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
        if declaration.group().shape() != Shape::Record {
            return Err(self.error(span, format!("`{}` has no fields", declaration.name)));
        }
        let TypeKind::Rec(declared_fields) = &declaration.kind else {
            unreachable!("a group with fields is a rec");
        };
        let name = declaration.name.clone();
        let parameters = declaration.parameters.clone();
        let declared: Vec<(String, bool, Type)> = declared_fields
            .iter()
            .map(|field| {
                (
                    field.name.clone(),
                    field.public,
                    substitute(
                        &field.ty,
                        &self.program.types,
                        &parameters,
                        Binding::Complete(arguments),
                    ),
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
            if declaration.group().shape() != Shape::Variant {
                return Err(self.error(span, format!("`{}` is not an enum", declaration.name)));
            }
            let TypeKind::Enum(variants) = &declaration.kind else {
                unreachable!("a group with variants is an enum");
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
                    crate::compiler::messages::count(declared.len(), "argument"),
                    payload.len()
                ),
            ));
        }
        for (argument, required) in payload.iter().zip(&declared) {
            let required = substitute(
                required,
                &self.program.types,
                &parameters,
                Binding::Complete(arguments),
            );
            self.require(argument, &required)?;
        }
        Ok(())
    }
}
