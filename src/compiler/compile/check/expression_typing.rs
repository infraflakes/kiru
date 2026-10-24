//! Typing expressions in one declaration body.
//!
//! One relation types every expression: `expression(expression, expected)`
//! derives an expression's type and, when the position asks for one, checks it
//! against that type. A shape literal takes its type from the expected type;
//! `fits` is the leaf equality. A function's return type is declared, so a
//! return is checked against it rather than accumulated, and a call that
//! produces no value may only stand as a statement.

use crate::compiler::model::{
    Binding, DeclarationId, Expression, Field, Function, Group, MatchBody, NamespaceId,
    RuntimeShape, Shape, Site, StringPart, Type, TypeHead, TypeId, TypeKind, bound_reachable,
    first_duplicate, fits, has_parameter, join_path, rec_field, substitute, unify,
};
use crate::compiler::operator_facts::BinaryOperands;
use crate::compiler::{
    Diagnostic, duplicate_field, expected_instead, native_used_as_value, value_called,
};
use crate::syntax::{BinaryOp, Span, UnaryOp};

use super::statement_check::Checker;

/// Whether an expression is a record literal, which has no type of its own and
/// takes it from the position it is checked against. The expression's shape is
/// a model fact, so the checker reads it rather than matching a node form.
fn is_record_literal(expression: &Expression) -> bool {
    expression.shape() == Some(Shape::Record)
}

/// The rejection a family that names no reachable bound function: a bare
/// method name has no method for the value, a qualified one is the unknown
/// name the pools could not resolve. The wording is the linker's, which
/// reported both before dispatch moved into the check.
fn family_rejected(namespace: Option<NamespaceId>, path: &[String]) -> String {
    match namespace {
        None => format!(
            "no method `{}` for this value",
            path.last().expect("a family names itself")
        ),
        Some(_) => format!("unknown name `{}`", join_path(path)),
    }
}

impl<'a> Checker<'a> {
    /// The type of a function value: the declaration's own type, which is a
    /// function type. A generic function has no one type, so it cannot be a
    /// value.
    fn function_value(&self, declaration: DeclarationId, span: Span) -> Result<Type, Diagnostic> {
        let function = self
            .model
            .declaration(declaration)
            .function()
            .expect("a function value names a function");
        if !function.type_parameters.is_empty() {
            return Err(self.error(
                span,
                format!(
                    "`{}` is generic; give it a type annotation to use it as a value",
                    self.model.display(declaration)
                ),
            ));
        }
        Ok(self.model.declaration(declaration).ty.clone())
    }

    /// The type of a function literal: its parameter declarations' resolved
    /// types plus the return its own annotation declares. A named function's
    /// type is its declaration's, so this reads only a literal's own parts.
    fn literal_type(&self, function: &Function) -> Type {
        Type::Function {
            parameters: function
                .parameters
                .iter()
                .map(|parameter| self.model.declaration(*parameter).ty.clone())
                .collect(),
            returns: function.declared_return.clone().map(Box::new),
        }
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
                // A record literal takes its type from the position, which must
                // be a declared type that is written as a record. The runtime
                // list declares that it has no literal shape, so it never takes
                // one; a declared type whose shape is not a record is rejected
                // by `map_against` with its own diagnostic.
                Some(Type::Named(id, arguments))
                    if self.model.type_decl(*id).shape() != Shape::None =>
                {
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
                    && *id == *ty
                {
                    self.variant_against(*ty, *index, arguments, payload, *span)?;
                    Ok(Some(Type::Named(*id, arguments.clone())))
                } else {
                    if !self.model.type_decl(*ty).parameters.is_empty() {
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
            // A family callee is dispatched with its call's arguments, which
            // `call` does before this walk reads the callee's kind here.
            Expression::Method { .. } => {
                unreachable!("a family callee is dispatched with its call's arguments")
            }
            // A function literal is a value whose type is a function type. Its
            // body is proved here, where it is written, so a captured loop
            // variable or pattern binding keeps the type the enclosing walk
            // knows.
            Expression::Function(function) => {
                self.check_function(function, function.declared_return.clone())?;
                Ok(Some(self.literal_type(function)))
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
                        let declaration = self.model.type_decl(id);
                        if declaration.group().has_variants() {
                            return Err(self.error(
                                *name_span,
                                format!("`{}` is an enum; match it", declaration.name),
                            ));
                        }
                        let Some(field) = rec_field(self.model.types(), id, name) else {
                            let message = format!("`{}` has no field `{name}`", declaration.name);
                            return Err(self.error(*name_span, message));
                        };
                        self.check_member(id, name, field.public, *name_span)?;
                        let ty = substitute(
                            &field.ty,
                            self.model.types(),
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
            (Some(actual), Some(required)) if !fits(actual, required, self.model.types()) => {
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

    /// The type of an expression from its declarations alone, without proving
    /// anything. This is the derivation the checking mode also performs: a
    /// literal has its scalar type, a name has its declared type, a field has
    /// its declared and substituted field type, a call has its callee's
    /// declared return type, an operator has the result its facts declare, a
    /// variant names its declaration, and a record literal has no type of its
    /// own. A type parameter the position leaves open stays open; only its
    /// head matters to method dispatch, and a head never depends on the
    /// arguments.
    pub(in crate::compiler::compile::check) fn derived_type(
        &self,
        expression: &Expression,
    ) -> Option<Type> {
        match expression {
            Expression::Text { .. } | Expression::Interpolated { .. } => Some(Type::Text),
            Expression::Bool { .. } => Some(Type::Bool),
            Expression::Int { .. } => Some(Type::Int),
            Expression::Map { .. } => None,
            Expression::Variant { ty, .. } => Some(Type::Named(*ty, Vec::new())),
            Expression::Reference { declaration, .. } => Some(self.kind_of(*declaration)),
            Expression::Call {
                callee, arguments, ..
            } => self.derived_call(callee, arguments),
            // A family callee is dispatched with its call's arguments, which
            // `derived_call` does before reading the callee's type here.
            Expression::Method { .. } => {
                unreachable!("a family callee is dispatched with its call's arguments")
            }
            Expression::Field { target, name, .. } => {
                let target = self.derived_type(target)?;
                self.field_type(&target, name)
            }
            Expression::Binary { op, .. } => Some(op.facts().result),
            Expression::Unary { op, .. } => Some(match op {
                UnaryOp::Negate => Type::Int,
                UnaryOp::Not => Type::Bool,
            }),
            // Every arm of a valid match has one type, so any expression arm
            // names it. The checker proves that the arms agree.
            Expression::Match { cases, default, .. } => {
                for arm in cases {
                    if let MatchBody::Expression(expression) = &arm.body {
                        return self.derived_type(expression);
                    }
                }
                match default {
                    Some(MatchBody::Expression(expression)) => self.derived_type(expression),
                    _ => None,
                }
            }
            // A function literal is a value, and no method binds to one.
            Expression::Function(_) => None,
        }
    }

    /// The declared return type of a call, read from the callee's declaration
    /// without binding its type parameters. A call through a value reads the
    /// return type of the value's function type. A family callee resolves the
    /// way the checked dispatch does, purely: the receiver's head selects the
    /// bound function, and an ambiguous or unreachable one derives no type —
    /// the checked dispatch reports those, at the walk's own order.
    fn derived_call(&self, callee: &Expression, arguments: &[Expression]) -> Option<Type> {
        if let Expression::Reference { declaration, .. } = callee
            && let Some(return_type) = self
                .model
                .declaration(*declaration)
                .return_type(self.model.types())
        {
            return Some(return_type);
        }
        if let Expression::Method {
            namespace,
            path,
            span,
        } = callee
        {
            let head = arguments
                .first()
                .and_then(|receiver| self.derived_type(receiver))?
                .head(self.model.types());
            let declaration = self
                .select_method(*namespace, path, Some(head), *span)
                .ok()
                .flatten()?;
            return self
                .model
                .declaration(declaration)
                .return_type(self.model.types());
        }
        self.derived_type(callee)?.return_type(self.model.types())
    }

    /// The type of a declared field of `target`, substituting the declared
    /// type's parameters with the target's arguments.
    fn field_type(&self, target: &Type, name: &str) -> Option<Type> {
        let Type::Named(id, arguments) = target else {
            return None;
        };
        let field = rec_field(self.model.types(), *id, name)?;
        Some(substitute(
            &field.ty,
            self.model.types(),
            &self.model.type_decl(*id).parameters,
            Binding::Complete(arguments),
        ))
    }

    /// The type of a binary operator. The operator's own facts say what its
    /// operands must be and what it produces; this walk only enforces that fact
    /// against the two operands.
    fn binary(
        &mut self,
        op: BinaryOp,
        left: &Expression,
        right: &Expression,
    ) -> Result<Option<Type>, Diagnostic> {
        let facts = op.facts();
        match facts.operands {
            BinaryOperands::Fixed(operand) => {
                self.require(left, &operand)?;
                self.require(right, &operand)?;
            }
            BinaryOperands::Ordered => {
                let Some(left_type) = self.expression(left, None)? else {
                    return Err(self.error(left.span(), expected_instead("int or txt", "no value")));
                };
                self.require(right, &left_type)?;
                if !matches!(left_type, Type::Int | Type::Text) {
                    return Err(self.error(
                        left.span(),
                        format!(
                            "`{}` cannot be ordered; compare `int` or `txt`",
                            self.type_name(&left_type)
                        ),
                    ));
                }
            }
            BinaryOperands::Same => {
                // A record literal takes its type from the other operand, so
                // equality reads the same whichever side the declared value is.
                let (typed, checked) = if is_record_literal(left) && !is_record_literal(right) {
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
            }
        }
        Ok(Some(facts.result))
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
        match ty.runtime_shape(self.model.types()) {
            RuntimeShape::Text | RuntimeShape::Bool | RuntimeShape::Int => return Ok(()),
            RuntimeShape::Record
            | RuntimeShape::List
            | RuntimeShape::Variant
            | RuntimeShape::Function
            | RuntimeShape::Parameter
            | RuntimeShape::Nothing => {}
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
        if self.model.declaration(declaration).is_native() {
            return Err(self.error(span, native_used_as_value(&self.model.display(declaration))));
        }
        if self.model.declaration(declaration).function().is_some() {
            return Ok(Some(self.function_value(declaration, span)?));
        }
        Ok(Some(self.kind_of(declaration)))
    }

    /// The type of an application. The callee is an expression and its type
    /// decides: a value of a function type is called through its value, a
    /// reference to a declared callable names the function its declaration
    /// holds, and a family callee is the method dispatch the check owns. The
    /// one distinction is generic instantiation and diagnostics: a declared
    /// generic function's type parameters are bound by the call and by the
    /// expected type, and its name labels an argument error. A native has no
    /// value form and is the one callee dispatched by index.
    fn call(
        &mut self,
        callee: &Expression,
        callee_span: Span,
        arguments: &[Expression],
        expected: Option<&Type>,
    ) -> Result<Option<Type>, Diagnostic> {
        let family = match callee {
            Expression::Reference { declaration, .. }
                if self.model.declaration(*declaration).is_callable() =>
            {
                let declaration = *declaration;
                return self.direct_call(declaration, callee_span, arguments, expected);
            }
            Expression::Method {
                namespace, path, ..
            } => Some((*namespace, path.clone())),
            _ => None,
        };
        if let Some((namespace, path)) = family {
            return self.method_call(namespace, &path, callee_span, arguments, expected);
        }
        self.call_value(callee, arguments)
    }

    /// The check's method dispatch: the one pass that types every expression
    /// computes the receiver's type here, and the head of that type selects
    /// the one bound function the family names. The family stays the family
    /// the linker wrote; the selected target is recorded in `Facts`, and every
    /// later pass reads the call's target from there.
    fn method_call(
        &mut self,
        namespace: Option<NamespaceId>,
        path: &[String],
        callee_span: Span,
        arguments: &[Expression],
        expected: Option<&Type>,
    ) -> Result<Option<Type>, Diagnostic> {
        let head = arguments
            .first()
            .and_then(|receiver| self.derived_type(receiver))
            .map(|ty| ty.head(self.model.types()));
        let Some(declaration) = self.select_method(namespace, path, head, callee_span)? else {
            return Err(self.error(callee_span, family_rejected(namespace, path)));
        };
        // Record the target before checking the arguments: reachability reads
        // the fact even when one argument later fails, and a reachable method
        // must stay reachable when the call is otherwise reported.
        self.facts
            .set_method_target(Site::new(self.owner, callee_span), declaration);
        self.direct_call(declaration, callee_span, arguments, expected)
    }

    /// The bound function the family names for a receiver head, under the one
    /// visibility rule. A bare family searches every namespace's method pool;
    /// a qualified one the namespace its path named. `None` when no reachable
    /// method fits; two are the ambiguity a value's family reports.
    fn select_method(
        &self,
        namespace: Option<NamespaceId>,
        path: &[String],
        head: Option<TypeHead>,
        span: Span,
    ) -> Result<Option<DeclarationId>, Diagnostic> {
        let (Some(head), Some(name)) = (head, path.last()) else {
            return Ok(None);
        };
        let Some(symbol) = self.model.intern().resolve(name) else {
            return Ok(None);
        };
        let found: Vec<DeclarationId> = match namespace {
            Some(namespace) => self
                .model
                .namespace(namespace)
                .methods
                .get(head, symbol)
                .into_iter()
                .collect(),
            None => self
                .model
                .namespaces()
                .iter()
                .filter_map(|space| space.methods.get(head, symbol))
                .collect(),
        };
        let visible: Vec<DeclarationId> = found
            .into_iter()
            .filter(|&declaration| self.method_reachable(declaration, head))
            .collect();
        match visible.as_slice() {
            [] => Ok(None),
            [declaration] => Ok(Some(*declaration)),
            _ => Err(self.error(span, format!("`{name}` is ambiguous for this value"))),
        }
    }

    /// Whether the walk's namespace may reach a bound function filed under
    /// `head`: the function's `pub` and, across namespaces, its type's `pub`.
    /// The one gate the linker's registration already applied.
    fn method_reachable(&self, declaration: DeclarationId, head: TypeHead) -> bool {
        let node = self.model.declaration(declaration);
        let type_public = match head {
            TypeHead::Named(id) => Some(self.model.type_decl(id).public),
            _ => None,
        };
        bound_reachable(node.public, type_public, node.namespace, self.namespace)
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
            let node = self.model.declaration(callee);
            let returns = node.return_type(self.model.types());
            let type_parameters = node.type_parameters(self.model.types());
            let declared = match node.ty.group(self.model.types()) {
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
                self.model.types(),
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
            if has_parameter(parameter, self.model.types()) {
                let mut hint_solution = solution.clone();
                for (slot, expected) in hint_solution.iter_mut().zip(&expected_bindings) {
                    if slot.is_none() {
                        *slot = expected.clone();
                    }
                }
                let hint_bound = substitute(
                    parameter,
                    self.model.types(),
                    &type_parameters,
                    Binding::Partial(&hint_solution),
                );
                let hint = (!has_parameter(&hint_bound, self.model.types())
                    && (parameter.group(self.model.types()) != Group::Parameter
                        || is_record_literal(argument)))
                .then_some(&hint_bound);
                let Some(actual) = self.expression(argument, hint)? else {
                    return Err(self.error(
                        argument.span(),
                        format!(
                            "`{}` cannot take an argument that produces no value",
                            self.model.display(callee)
                        ),
                    ));
                };
                if !unify(
                    parameter,
                    &actual,
                    self.model.types(),
                    &type_parameters,
                    &mut solution,
                ) {
                    return Err(self.error(
                        argument.span(),
                        format!(
                            "`{}` cannot take an argument of type {}",
                            self.model.display(callee),
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
        // The checker demands a binding for every parameter.
        for (index, bound) in solution.iter().enumerate() {
            if bound.is_none() {
                return Err(self.error(
                    callee_span,
                    format!(
                        "cannot infer the type parameter `{}` of `{}`; add a type annotation",
                        type_parameters[index],
                        self.model.display(callee)
                    ),
                ));
            }
        }
        for (argument, parameter) in arguments.iter().zip(&declared) {
            let required = substitute(
                parameter,
                self.model.types(),
                &type_parameters,
                Binding::Partial(&solution),
            );
            self.require(argument, &required)?;
        }
        Ok(returns.map(|returns| {
            substitute(
                &returns,
                self.model.types(),
                &type_parameters,
                Binding::Partial(&solution),
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
                let declaration = *declaration;
                let span = callee.span();
                return Err(self.error(span, value_called(&self.model.display(declaration))));
            }
            return Err(self.error(callee.span(), "a call's callee must produce a value"));
        };
        if callee_type.group(self.model.types()) != Group::Function {
            if let Expression::Reference { declaration, .. } = callee {
                let declaration = *declaration;
                let span = callee.span();
                return Err(self.error(span, value_called(&self.model.display(declaration))));
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
                    crate::compiler::diagnostic::count(parameters.len(), "argument"),
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
                    self.model.display(callee),
                    crate::compiler::diagnostic::count(expected, "argument"),
                    arguments.len()
                ),
            ));
        }
        Ok(())
    }

    /// Require a value of a type, returning that type. The value-position entry
    /// to the one relation: `expression` with the position's declared type. An
    /// expression that produces no value is rejected here.
    pub(in crate::compiler::compile::check) fn require(
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
        let declaration = self.model.type_decl(id);
        if declaration.shape() != Shape::Record {
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
                        self.model.types(),
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
            self.check_member(id, field_name, *public, field.name_span)?;
            self.require(&field.value, field_ty)?;
        }
        for field in fields.iter() {
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
            let declaration = self.model.type_decl(id);
            if declaration.shape() != Shape::Variant {
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
        self.check_member(id, &variant_name, public, span)?;
        let subject = format!("{name}.{variant_name}");
        if let Some(message) = crate::compiler::diagnostic::variant_payload_count(
            &subject,
            declared.len(),
            payload.len(),
            "argument",
        ) {
            return Err(self.error(span, message));
        }
        for (argument, required) in payload.iter().zip(&declared) {
            let required = substitute(
                required,
                self.model.types(),
                &parameters,
                Binding::Complete(arguments),
            );
            self.require(argument, &required)?;
        }
        Ok(())
    }
}
