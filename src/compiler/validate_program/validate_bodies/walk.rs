//! The validation walk over one declaration body.
//!
//! Names are already edges, so the walker only reads and writes node kinds.
//! Kinds are derived bottom-up: the walk returns the kind of each expression
//! it visits and records the local binding kinds. A function's return kind is
//! declared, so a return is checked against it rather than accumulated. The
//! usage matrix decides where each kind may stand.

use std::collections::HashMap;

use crate::compiler::{Diagnostic, expected_instead, function_used_as_value, value_called};
use crate::model::{
    BindingKind, DeclarationId, DeclarationKind, Expression, Field, FileId, Kind, Position,
    Program, Statement, Usage, fits,
};
use crate::syntax::Span;

use super::flow::Flow;
use super::patterns::{atoms_equal, duplicate_pattern_message, is_atom};

/// The walker over one body. Names are already edges, so it only reads and
/// writes node kinds.
pub(in crate::compiler::validate_program) struct Walk<'a> {
    program: &'a Program,
    file: FileId,
    pub(super) locals: HashMap<DeclarationId, Kind>,
    /// The name of the function being walked. A module value walk has none,
    /// and a `return` cannot occur in one.
    function_name: Option<&'a str>,
    /// The function's declared return kind.
    declared_kind: Kind,
    /// Whether the walk is inside a `for` body, where `break` is allowed.
    in_loop: bool,
}

impl<'a> Walk<'a> {
    /// A walk over a module value initializer. It never visits a statement.
    pub(in crate::compiler::validate_program) fn new(program: &'a Program, file: FileId) -> Self {
        Self {
            program,
            file,
            locals: HashMap::new(),
            function_name: None,
            declared_kind: Kind::Nothing,
            in_loop: false,
        }
    }

    /// A walk over a function body. A return is checked against the declared
    /// kind.
    pub(in crate::compiler::validate_program) fn for_function(
        program: &'a Program,
        file: FileId,
        function_name: &'a str,
        declared_kind: Kind,
    ) -> Self {
        Self {
            program,
            file,
            locals: HashMap::new(),
            function_name: Some(function_name),
            declared_kind,
            in_loop: false,
        }
    }

    fn error(&self, span: Span, message: impl Into<String>) -> Diagnostic {
        Diagnostic::new(&self.program.file(self.file).path, span, message)
    }

    /// The kind of a binding, a parameter, or a module value.
    fn kind_of(&self, id: DeclarationId) -> Kind {
        self.locals
            .get(&id)
            .copied()
            .or(self.program.declaration(id).derived.kind)
            .expect("every visited binding and callable carries a kind")
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
                self.bind(*declaration, value)?;
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
            Statement::Expression(expression) => {
                // A bare statement must do something: call a function or
                // native. A value on its own is dead text, so the shape is
                // rejected before the kind.
                if !matches!(expression, Expression::Call { .. }) {
                    return Err(self.error(expression.span(), "a statement must be a call"));
                }
                self.require(expression, Position::Statement)?;
                Ok(self.call_flow(expression))
            }
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
            Statement::Switch {
                subject,
                cases,
                default,
                ..
            } => {
                self.require(subject, Position::CasePattern)?;
                let mut seen: Vec<&Expression> = Vec::new();
                let mut flow = Flow::STOPS;
                for case in cases {
                    if !is_atom(&case.pattern) {
                        return Err(self.error(
                            case.pattern.span(),
                            "a case arm is a literal, a name, or a field path",
                        ));
                    }
                    // Two arms are duplicates when they are the same atom:
                    // equal literal text, the same declaration, or the same
                    // field path. Report the second occurrence.
                    if seen.iter().any(|other| atoms_equal(other, &case.pattern)) {
                        return Err(self.error(
                            case.pattern.span(),
                            duplicate_pattern_message(&case.pattern),
                        ));
                    }
                    seen.push(&case.pattern);
                    self.require(&case.pattern, Position::CasePattern)?;
                    flow = flow.union(self.statements(&case.body)?);
                }
                match default {
                    Some(default) => flow = flow.union(self.statements(default)?),
                    None => flow = flow.union(Flow::FALLS),
                }
                Ok(flow)
            }
            Statement::ForEach {
                item,
                iterable,
                body,
                ..
            } => {
                self.require(iterable, Position::Iterable)?;
                self.locals.insert(*item, Kind::Text);
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

    /// Bind a local `txt` or `rec` and record its kind.
    fn bind(&mut self, declaration: DeclarationId, value: &Expression) -> Result<(), Diagnostic> {
        let position = match &self.program.declaration(declaration).kind {
            DeclarationKind::Binding(BindingKind::Text { .. }) => Position::TextBinding,
            DeclarationKind::Binding(BindingKind::Record { .. }) => Position::RecordBinding,
            DeclarationKind::Binding(BindingKind::List { .. }) => Position::ListBinding,
            other => {
                unreachable!("a bind statement targets a text or record binding, found {other:?}")
            }
        };
        let actual = self.require(value, position)?;
        self.locals.insert(declaration, actual);
        Ok(())
    }

    fn assign(
        &mut self,
        declaration: DeclarationId,
        name_span: Span,
        value: &Expression,
    ) -> Result<(), Diagnostic> {
        match &self.program.declaration(declaration).kind {
            DeclarationKind::Binding(binding) if !binding.mutable() => {
                let name = &self.program.declaration(declaration).name;
                Err(self.error(
                    name_span,
                    format!("`{name}` is not mutable; declare it `mut`"),
                ))
            }
            DeclarationKind::Binding(_) => {
                let binding_kind = self.kind_of(declaration);
                self.require_kind(value, binding_kind)
            }
            DeclarationKind::Text(_) | DeclarationKind::Record(_) | DeclarationKind::List(_) => {
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
    /// mutable binding whose kind is a record; the value must be text.
    fn field_assign(
        &mut self,
        declaration: DeclarationId,
        name_span: Span,
        value: &Expression,
    ) -> Result<(), Diagnostic> {
        let node = self.program.declaration(declaration);
        let name = node.name.clone();
        match &node.kind {
            DeclarationKind::Binding(binding) if !binding.mutable() => {
                return Err(self.error(
                    name_span,
                    format!("`{name}` is not mutable; declare it `mut`"),
                ));
            }
            DeclarationKind::Binding(_) => {}
            DeclarationKind::Text(_) | DeclarationKind::Record(_) | DeclarationKind::List(_) => {
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
        if self.kind_of(declaration) != Kind::Record {
            return Err(self.error(
                name_span,
                format!("`{name}` is not a record and has no fields"),
            ));
        }
        self.require(value, Position::RecordField)?;
        Ok(())
    }

    /// Enforce the return rules of the enclosing function. A value return is
    /// only for a function declared `-> txt` or `-> rec`, and its value must fit
    /// that kind; a bare return is only for a function declared with no return
    /// kind.
    fn return_statement(
        &mut self,
        value: Option<&Expression>,
        span: Span,
    ) -> Result<(), Diagnostic> {
        let Some(name) = self.function_name else {
            return Err(self.error(span, "`return` is not allowed here"));
        };
        match value {
            Some(value) if self.declared_kind == Kind::Nothing => Err(self.error(
                value.span(),
                format!("`{name}` is declared to return nothing, so `return` cannot carry a value"),
            )),
            Some(value) => self.require_kind(value, self.declared_kind),
            None if self.declared_kind != Kind::Nothing => Err(self.error(
                span,
                format!(
                    "`{name}` is declared to return {}, so `return` must carry a value",
                    self.declared_kind.name()
                ),
            )),
            None => Ok(()),
        }
    }

    pub(super) fn fields(&mut self, fields: &[Field]) -> Result<(), Diagnostic> {
        for field in fields {
            self.require(&field.value, Position::RecordField)?;
        }
        Ok(())
    }

    fn expression(&mut self, expression: &Expression) -> Result<Kind, Diagnostic> {
        match expression {
            Expression::Text { .. } => Ok(Kind::Text),
            Expression::Record { fields, .. } => {
                self.fields(fields)?;
                Ok(Kind::Record)
            }
            Expression::List { elements, .. } => {
                for element in elements {
                    self.require(element, Position::ListElement)?;
                }
                Ok(Kind::List)
            }
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
                self.require(target, Position::RecordBinding)?;
                Ok(Kind::Text)
            }
            Expression::Add { left, right, .. } => {
                self.require(left, Position::TextBinding)?;
                self.require(right, Position::TextBinding)?;
                Ok(Kind::Text)
            }
        }
    }

    fn reference(&mut self, declaration: DeclarationId, span: Span) -> Result<Kind, Diagnostic> {
        match &self.program.declaration(declaration).kind {
            DeclarationKind::Binding(_)
            | DeclarationKind::Text(_)
            | DeclarationKind::Record(_)
            | DeclarationKind::List(_) => Ok(self.kind_of(declaration)),
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
    ) -> Result<Kind, Diagnostic> {
        if !matches!(
            self.program.declaration(callee).kind,
            DeclarationKind::Function(_) | DeclarationKind::Native(_)
        ) {
            return Err(self.error(callee_span, value_called(&self.program.display(callee))));
        }
        let parameter_count = self.program.declaration(callee).parameters.len();
        let returns = self
            .program
            .declaration(callee)
            .derived
            .kind
            .expect("every callable carries the kind its declaration fixed");
        self.arity(callee, callee_span, arguments, parameter_count)?;
        for (index, argument) in arguments.iter().enumerate() {
            let parameter = self.program.declaration(callee).parameters[index];
            let required = self.kind_of(parameter);
            self.require_kind(argument, required)?;
        }
        Ok(returns)
    }

    /// Require one argument to fit a parameter's kind. This is the one rule
    /// for every callable, so a native and a user function are checked the same
    /// way.
    fn require_kind(&mut self, argument: &Expression, required: Kind) -> Result<(), Diagnostic> {
        self.require_usage(argument, required.usage())?;
        Ok(())
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

    /// Require a value whose kind fits a position, returning its kind. The
    /// position names the usage; the message names that requirement.
    pub(in crate::compiler::validate_program) fn require(
        &mut self,
        expression: &Expression,
        position: Position,
    ) -> Result<Kind, Diagnostic> {
        self.require_usage(expression, position.required_usage())
    }

    /// Require an expression whose kind fits a usage, returning its kind. This
    /// is the one compatibility check; every caller names its requirement as a
    /// kind or a position and lands here.
    fn require_usage(
        &mut self,
        expression: &Expression,
        required: Usage,
    ) -> Result<Kind, Diagnostic> {
        let actual = self.expression(expression)?;
        if fits(actual.usage(), required) {
            return Ok(actual);
        }
        Err(self.error(
            expression.span(),
            expected_instead(required.name(), actual.name()),
        ))
    }
}
