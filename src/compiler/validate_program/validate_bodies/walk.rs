//! The validation walk over one declaration body.
//!
//! Names are already edges, so the walker only reads and writes node types.
//! Types are derived bottom-up: the walk returns the type of each expression
//! it visits. A function's return type is declared, so a return is checked
//! against it rather than accumulated. Every value position asks for one type
//! through `require`, and a call that produces no value may only stand as a
//! statement.

use crate::compiler::{Diagnostic, expected_instead, function_used_as_value, value_called};
use crate::model::{
    Binding, DeclarationId, DeclarationKind, Expression, Field, FileId, MatchArm, MatchBody,
    Program, Statement, StringPart,
};
use crate::native_registry::native_row;
use crate::syntax::Span;
use crate::types::{Type, fits};

use super::flow::Flow;
use super::patterns::{atoms_equal, duplicate_pattern_message, is_atom};

/// The walker over one body. Names are already edges, so it only reads and
/// writes node types.
pub(in crate::compiler::validate_program) struct Walk<'a> {
    program: &'a Program,
    file: FileId,
    /// The name of the function being walked. A module value walk has none,
    /// and a `return` cannot occur in one.
    function_name: Option<&'a str>,
    /// The function's declared return type: `Void` when it returns no value.
    return_type: Type,
    /// Whether the walk is inside a `for` body, where `break` is allowed.
    in_loop: bool,
}

impl<'a> Walk<'a> {
    /// A walk over a module value initializer. It never visits a statement.
    pub(in crate::compiler::validate_program) fn new(program: &'a Program, file: FileId) -> Self {
        Self {
            program,
            file,
            function_name: None,
            return_type: Type::Void,
            in_loop: false,
        }
    }

    /// A walk over a function body. A return is checked against the declared
    /// return type.
    pub(in crate::compiler::validate_program) fn for_function(
        program: &'a Program,
        file: FileId,
        function_name: &'a str,
        return_type: Type,
    ) -> Self {
        Self {
            program,
            file,
            function_name: Some(function_name),
            return_type,
            in_loop: false,
        }
    }

    fn error(&self, span: Span, message: impl Into<String>) -> Diagnostic {
        Diagnostic::new(&self.program.file(self.file).path, span, message)
    }

    /// The type of a binding or a module value.
    fn kind_of(&self, id: DeclarationId) -> Type {
        match &self.program.declaration(id).kind {
            DeclarationKind::Binding(binding) => binding.ty,
            DeclarationKind::Value { ty, .. } => *ty,
            other => unreachable!("a value node carries a type, found {other:?}"),
        }
    }

    /// The binding node a bind statement targets.
    fn binding_of(&self, id: DeclarationId) -> Binding {
        match &self.program.declaration(id).kind {
            DeclarationKind::Binding(binding) => *binding,
            other => unreachable!("a bind statement targets a binding, found {other:?}"),
        }
    }

    /// Validate a sequence of statements and return its flow. Every statement
    /// is validated, including the ones after a terminator; only the ones
    /// reachable from the start contribute to the flow.
    pub(in crate::compiler::validate_program) fn statements(
        &mut self,
        statements: &[Statement],
    ) -> Result<Flow, Diagnostic> {
        let mut flow = Flow::FALLS;
        for statement in statements {
            let next = self.statement(statement)?;
            if flow.falls {
                flow = Flow {
                    returns: flow.returns || next.returns,
                    falls: next.falls,
                    breaks: flow.breaks || next.breaks,
                };
            }
        }
        Ok(flow)
    }

    /// Validate one statement and return its flow.
    fn statement(&mut self, statement: &Statement) -> Result<Flow, Diagnostic> {
        match statement {
            Statement::Bind {
                declaration, value, ..
            } => {
                let ty = self.binding_of(*declaration).ty;
                self.require(value, ty)?;
                Ok(Flow::FALLS)
            }
            Statement::Assign {
                declaration,
                name_span,
                value,
                ..
            } => {
                self.assign(*declaration, *name_span, value)?;
                Ok(Flow::FALLS)
            }
            Statement::FieldAssign {
                declaration,
                name_span,
                value,
                ..
            } => {
                self.field_assign(*declaration, *name_span, value)?;
                Ok(Flow::FALLS)
            }
            Statement::Expression(expression) => match expression {
                // A bare call runs for its effect; a call that halts ends the
                // path.
                Expression::Call { .. } => {
                    self.expression(expression)?;
                    Ok(self.call_flow(expression))
                }
                // A bare match runs its taken arm; its flow is the arms' union.
                Expression::Match {
                    subject,
                    cases,
                    default,
                    ..
                } => self.match_statement(subject, cases, default),
                // Any other bare value is dead, so the shape is rejected.
                _ => Err(self.error(expression.span(), "a statement must be a call")),
            },
            Statement::Return { value, span } => {
                self.return_statement(value.as_ref(), *span)?;
                Ok(Flow::RETURNS)
            }
            Statement::Panic { .. } => Ok(Flow::STOPS),
            Statement::Async { call, .. } => {
                if !matches!(call, Expression::Call { .. }) {
                    return Err(self.error(call.span(), "`async` takes a call"));
                }
                self.expression(call)?;
                Ok(Flow::FALLS)
            }
            Statement::Wait { .. } => Ok(Flow::FALLS),
            Statement::ForEach { iterable, body, .. } => {
                self.require(iterable, Type::List)?;
                let was_in_loop = self.in_loop;
                self.in_loop = true;
                let body_flow = self.statements(body);
                self.in_loop = was_in_loop;
                let body_flow = body_flow?;
                // A list `for` may run zero times, so it always falls through; a
                // returning path in the body still returns. Its own `break`
                // exits the loop and then falls through, so it is absorbed.
                Ok(Flow {
                    returns: body_flow.returns,
                    falls: true,
                    breaks: false,
                })
            }
            Statement::Forever { body, .. } => {
                let was_in_loop = self.in_loop;
                self.in_loop = true;
                let body_flow = self.statements(body);
                self.in_loop = was_in_loop;
                let body_flow = body_flow?;
                // An unbounded `for` falls through only when its body can
                // break; a returning path in the body still returns.
                Ok(Flow {
                    returns: body_flow.returns,
                    falls: body_flow.breaks,
                    breaks: false,
                })
            }
            Statement::Break { span } => {
                if !self.in_loop {
                    return Err(self.error(*span, "`break` is only allowed inside a loop"));
                }
                Ok(Flow::BREAKS)
            }
        }
    }

    /// The flow of a bare call statement: it stops the run when its callee
    /// does.
    fn call_flow(&self, expression: &Expression) -> Flow {
        if let Expression::Call { callee, .. } = expression
            && self.program.declaration(*callee).derived.halts
        {
            return Flow::STOPS;
        }
        Flow::FALLS
    }

    fn assign(
        &mut self,
        declaration: DeclarationId,
        name_span: Span,
        value: &Expression,
    ) -> Result<(), Diagnostic> {
        match &self.program.declaration(declaration).kind {
            DeclarationKind::Binding(binding) if !binding.mutable => {
                let name = &self.program.declaration(declaration).name;
                Err(self.error(
                    name_span,
                    format!("`{name}` is not mutable; declare it `mut`"),
                ))
            }
            DeclarationKind::Binding(_) => {
                let ty = self.kind_of(declaration);
                self.require(value, ty)?;
                Ok(())
            }
            DeclarationKind::Value { .. } => {
                let name = &self.program.declaration(declaration).name;
                Err(self.error(
                    name_span,
                    format!("`{name}` is a module-level constant and cannot be assigned"),
                ))
            }
            other => {
                unreachable!("an assignment target is a binding or a module value, found {other:?}")
            }
        }
    }

    /// Assign one field of a mutable record binding. The target must be a
    /// mutable binding whose type is a record; the value must be text.
    fn field_assign(
        &mut self,
        declaration: DeclarationId,
        name_span: Span,
        value: &Expression,
    ) -> Result<(), Diagnostic> {
        let node = self.program.declaration(declaration);
        let name = node.name.clone();
        match &node.kind {
            DeclarationKind::Binding(binding) if !binding.mutable => {
                return Err(self.error(
                    name_span,
                    format!("`{name}` is not mutable; declare it `mut`"),
                ));
            }
            DeclarationKind::Binding(_) => {}
            DeclarationKind::Value { .. } => {
                return Err(self.error(
                    name_span,
                    format!("`{name}` is a module-level constant and cannot be assigned"),
                ));
            }
            other => {
                unreachable!(
                    "a field assignment target is a binding or a module value, found {other:?}"
                )
            }
        }
        if self.kind_of(declaration) != Type::Record {
            return Err(self.error(
                name_span,
                format!("`{name}` is not a record and has no fields"),
            ));
        }
        self.require(value, Type::Text)?;
        Ok(())
    }

    /// Enforce the return rules of the enclosing function. A value return is
    /// only for a function declared `-> txt` or `-> rec`, and its value must
    /// fit that type; a bare return is only for a function declared with no
    /// return type.
    fn return_statement(
        &mut self,
        value: Option<&Expression>,
        span: Span,
    ) -> Result<(), Diagnostic> {
        let Some(name) = self.function_name else {
            return Err(self.error(span, "`return` is not allowed here"));
        };
        match (value, self.return_type) {
            (Some(value), Type::Void) => Err(self.error(
                value.span(),
                format!("`{name}` is declared to return nothing, so `return` cannot carry a value"),
            )),
            (Some(value), return_type) => {
                self.require(value, return_type)?;
                Ok(())
            }
            (None, Type::Void) => Ok(()),
            (None, return_type) => Err(self.error(
                span,
                format!(
                    "`{name}` is declared to return {}, so `return` must carry a value",
                    return_type.name()
                ),
            )),
        }
    }

    pub(super) fn fields(&mut self, fields: &[Field]) -> Result<(), Diagnostic> {
        for field in fields {
            self.require(&field.value, Type::Text)?;
        }
        Ok(())
    }

    /// Derive the type of an expression. A call that returns no value has type
    /// `Void`, so every expression has a type.
    fn expression(&mut self, expression: &Expression) -> Result<Type, Diagnostic> {
        match expression {
            Expression::Text { .. } => Ok(Type::Text),
            Expression::Record { fields, .. } => {
                self.fields(fields)?;
                Ok(Type::Record)
            }
            Expression::List { elements, .. } => {
                for element in elements {
                    self.require(element, Type::Text)?;
                }
                Ok(Type::List)
            }
            Expression::Interpolated { parts, .. } => {
                for part in parts {
                    if let StringPart::Expression(expression) = part {
                        self.require(expression, Type::Text)?;
                    }
                }
                Ok(Type::Text)
            }
            Expression::Match {
                subject,
                cases,
                default,
                ..
            } => self.match_expression(subject, cases, default),
            Expression::Reference {
                declaration, span, ..
            } => self.reference(*declaration, *span),
            Expression::Call {
                callee,
                callee_span,
                arguments,
                ..
            } => self.call(*callee, *callee_span, arguments),
            Expression::Field { target, .. } => {
                self.require(target, Type::Record)?;
                Ok(Type::Text)
            }
            Expression::Add { left, right, .. } => {
                self.require(left, Type::Text)?;
                self.require(right, Type::Text)?;
                Ok(Type::Text)
            }
        }
    }

    fn reference(&mut self, declaration: DeclarationId, span: Span) -> Result<Type, Diagnostic> {
        match &self.program.declaration(declaration).kind {
            DeclarationKind::Binding(_) | DeclarationKind::Value { .. } => {
                Ok(self.kind_of(declaration))
            }
            DeclarationKind::Function(_) | DeclarationKind::Native(_) => Err(self.error(
                span,
                function_used_as_value(&self.program.display(declaration)),
            )),
        }
    }

    fn call(
        &mut self,
        callee: DeclarationId,
        callee_span: Span,
        arguments: &[Expression],
    ) -> Result<Type, Diagnostic> {
        let returns = match &self.program.declaration(callee).kind {
            DeclarationKind::Function(function) => function.return_type,
            DeclarationKind::Native(native) => native_row(*native).returns,
            _ => {
                return Err(self.error(callee_span, value_called(&self.program.display(callee))));
            }
        };
        let parameter_count = self.program.declaration(callee).parameters.len();
        self.arity(callee, callee_span, arguments, parameter_count)?;
        for (index, argument) in arguments.iter().enumerate() {
            let parameter = self.program.declaration(callee).parameters[index];
            let required = self.kind_of(parameter);
            self.require(argument, required)?;
        }
        Ok(returns)
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
                    "`{}` takes {expected} arguments, found {}",
                    self.program.display(callee),
                    arguments.len()
                ),
            ));
        }
        Ok(())
    }

    /// Require a value of a type, returning that type. This is the one
    /// compatibility check; every caller names its requirement as a type. A
    /// value of the wrong type, including `Void`, is one generic mismatch.
    pub(in crate::compiler::validate_program) fn require(
        &mut self,
        expression: &Expression,
        required: Type,
    ) -> Result<Type, Diagnostic> {
        let actual = self.expression(expression)?;
        if fits(actual, required) {
            return Ok(actual);
        }
        Err(self.error(
            expression.span(),
            expected_instead(required.name(), actual.name()),
        ))
    }

    /// The type of a match in expression position: the common type of the arm
    /// expressions, or `Void` when the arms are blocks.
    fn match_expression(
        &mut self,
        subject: &Expression,
        cases: &[MatchArm],
        default: &Option<MatchBody>,
    ) -> Result<Type, Diagnostic> {
        self.require(subject, Type::Text)?;
        let mut seen: Vec<&Expression> = Vec::new();
        let mut result: Option<Type> = None;
        for arm in cases {
            self.match_pattern(&arm.pattern, &mut seen)?;
            let ty = self.match_body_type(&arm.body)?;
            if let Some(previous) = result
                && previous != ty
            {
                return Err(self.error(arm.span, expected_instead(previous.name(), ty.name())));
            }
            result = Some(ty);
        }
        if let Some(default) = default {
            let ty = self.match_body_type(default)?;
            if let Some(previous) = result
                && previous != ty
            {
                return Err(
                    self.error(subject.span(), expected_instead(previous.name(), ty.name()))
                );
            }
            result = Some(ty);
        }
        Ok(result.unwrap_or(Type::Void))
    }

    /// The flow of a match in statement position: the union of the taken arm's
    /// flow, plus a fall-through when there is no `_` arm.
    fn match_statement(
        &mut self,
        subject: &Expression,
        cases: &[MatchArm],
        default: &Option<MatchBody>,
    ) -> Result<Flow, Diagnostic> {
        self.require(subject, Type::Text)?;
        let mut seen: Vec<&Expression> = Vec::new();
        let mut flow = Flow::STOPS;
        for arm in cases {
            self.match_pattern(&arm.pattern, &mut seen)?;
            flow = flow.union(self.match_body_flow(&arm.body)?);
        }
        match default {
            Some(default) => flow = flow.union(self.match_body_flow(default)?),
            None => flow = flow.union(Flow::FALLS),
        }
        Ok(flow)
    }

    /// Validate one match arm's pattern: a text atom, not a repeat.
    fn match_pattern<'e>(
        &mut self,
        pattern: &'e Expression,
        seen: &mut Vec<&'e Expression>,
    ) -> Result<(), Diagnostic> {
        if !is_atom(pattern) {
            return Err(self.error(
                pattern.span(),
                "a match arm is a literal, a name, or a field path",
            ));
        }
        // Two arms are duplicates when they are the same atom: equal literal
        // text, the same declaration, or the same field path.
        if seen.iter().any(|other| atoms_equal(other, pattern)) {
            return Err(self.error(pattern.span(), duplicate_pattern_message(pattern)));
        }
        self.require(pattern, Type::Text)?;
        seen.push(pattern);
        Ok(())
    }

    /// The type of a match arm's body: an expression's type, or `Void` for a
    /// block.
    fn match_body_type(&mut self, body: &MatchBody) -> Result<Type, Diagnostic> {
        match body {
            MatchBody::Expression(expression) => self.expression(expression),
            MatchBody::Block(statements) => {
                self.statements(statements)?;
                Ok(Type::Void)
            }
        }
    }

    /// The flow of a match arm's body.
    fn match_body_flow(&mut self, body: &MatchBody) -> Result<Flow, Diagnostic> {
        match body {
            MatchBody::Expression(expression) => {
                self.expression(expression)?;
                Ok(Flow::FALLS)
            }
            MatchBody::Block(statements) => self.statements(statements),
        }
    }
}
