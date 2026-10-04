//! The validation walk over one declaration body.
//!
//! Names are already edges, so the walker only reads and writes node kinds.
//! Kinds are derived bottom-up: the walk returns the kind of each expression
//! it visits and records the local binding kinds and the function's return
//! kind. The usage matrix decides where each kind may stand.

use std::collections::HashMap;

use crate::compiler::{
    BindingKind, DeclarationId, DeclarationKind, Diagnostic, Expression, Field, FileId, Kind,
    Position, Program, Statement, Usage, expected_instead, fits, function_used_as_value,
    value_called,
};
use crate::syntax::Span;

/// The walker over one body. Names are already edges, so it only reads and
/// writes node kinds.
pub(super) struct Walk<'a> {
    program: &'a Program,
    file: FileId,
    pub(super) locals: HashMap<DeclarationId, Kind>,
    pub(super) return_kind: Option<Kind>,
}

impl<'a> Walk<'a> {
    pub(super) fn new(program: &'a Program, file: FileId) -> Self {
        Self {
            program,
            file,
            locals: HashMap::new(),
            return_kind: None,
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

    pub(super) fn statements(&mut self, statements: &[Statement]) -> Result<(), Diagnostic> {
        for statement in statements {
            match statement {
                Statement::Bind {
                    declaration, value, ..
                } => self.bind(*declaration, value)?,
                Statement::Assign {
                    declaration,
                    name_span,
                    value,
                    ..
                } => self.assign(*declaration, *name_span, value)?,
                Statement::Expression(expression) => {
                    // A bare statement must do something: call a function or
                    // native. A value on its own is dead text, so the shape is
                    // rejected before the kind.
                    if !matches!(expression, Expression::Call { .. }) {
                        return Err(self.error(expression.span(), "a statement must be a call"));
                    }
                    self.require(expression, Position::Statement)?;
                }
                Statement::Return { value, .. } => {
                    if let Some(value) = value {
                        self.return_value(value)?;
                    }
                }
                Statement::Panic { .. } => {}
                Statement::Async { call, .. } => {
                    if !matches!(call, Expression::Call { .. }) {
                        return Err(self.error(call.span(), "`async` takes a call"));
                    }
                    self.expression(call)?;
                }
                Statement::Wait { .. } => {}
                Statement::Switch {
                    subject,
                    cases,
                    default,
                    ..
                } => {
                    self.require(subject, Position::CasePattern)?;
                    let mut seen: Vec<&Expression> = Vec::new();
                    for case in cases {
                        // Two patterns are duplicates when they are the same
                        // shape: equal literal text, the same declaration, or
                        // the same callee with equal arguments. Report the
                        // second occurrence.
                        if seen
                            .iter()
                            .any(|other| structurally_equal(other, &case.pattern))
                        {
                            return Err(self.error(
                                case.pattern.span(),
                                duplicate_pattern_message(&case.pattern),
                            ));
                        }
                        seen.push(&case.pattern);
                        self.require(&case.pattern, Position::CasePattern)?;
                        self.statements(&case.body)?;
                    }
                    if let Some(default) = default {
                        self.statements(default)?;
                    }
                }
                Statement::Defer { body, .. } => {
                    self.statements(body)?;
                }
            }
        }
        Ok(())
    }

    /// Bind a local `txt` or `rec` and record its kind.
    fn bind(&mut self, declaration: DeclarationId, value: &Expression) -> Result<(), Diagnostic> {
        let position = match &self.program.declaration(declaration).kind {
            DeclarationKind::Binding(BindingKind::Text) => Position::TextBinding,
            DeclarationKind::Binding(BindingKind::Record) => Position::RecordBinding,
            _ => return Ok(()),
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
            DeclarationKind::Binding(BindingKind::Parameter) => {
                let name = &self.program.declaration(declaration).name;
                Err(self.error(
                    name_span,
                    format!("`{name}` is a parameter and is read-only"),
                ))
            }
            DeclarationKind::Binding(BindingKind::Text | BindingKind::Record) => {
                let binding_kind = self.kind_of(declaration);
                self.require_kind(value, binding_kind)
            }
            _ => {
                let name = &self.program.declaration(declaration).name;
                Err(self.error(
                    name_span,
                    format!("`{name}` is a module-level constant and cannot be assigned"),
                ))
            }
        }
    }

    /// Validate a returned expression and record the function's return kind.
    /// Every value return in one function must carry the same kind.
    fn return_value(&mut self, value: &Expression) -> Result<(), Diagnostic> {
        let actual = self.require(value, Position::Return)?;
        match self.return_kind {
            Some(existing) if existing != actual => Err(self.error(
                value.span(),
                expected_instead(existing.name(), actual.name()),
            )),
            _ => {
                self.return_kind = Some(actual);
                Ok(())
            }
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
            DeclarationKind::Binding(_) | DeclarationKind::Text(_) => Ok(self.kind_of(declaration)),
            DeclarationKind::Record(_) => Ok(Kind::Record),
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
            .unwrap_or(Kind::Nothing);
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
    pub(super) fn require(
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

/// Whether two patterns are the same expression: equal literal text, the same
/// declaration reference, the same callee with equal arguments, and so on.
/// Two structurally equal case patterns are a duplicate.
fn structurally_equal(left: &Expression, right: &Expression) -> bool {
    match (left, right) {
        (Expression::Text { value: a, .. }, Expression::Text { value: b, .. }) => a == b,
        (
            Expression::Reference { declaration: a, .. },
            Expression::Reference { declaration: b, .. },
        ) => a == b,
        (
            Expression::Call {
                callee: a,
                arguments: a_arguments,
                ..
            },
            Expression::Call {
                callee: b,
                arguments: b_arguments,
                ..
            },
        ) => a == b && arguments_equal(a_arguments, b_arguments),
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
        ) => a_name == b_name && structurally_equal(a, b),
        (
            Expression::Add {
                left: a_left,
                right: a_right,
                ..
            },
            Expression::Add {
                left: b_left,
                right: b_right,
                ..
            },
        ) => structurally_equal(a_left, b_left) && structurally_equal(a_right, b_right),
        (Expression::Record { fields: a, .. }, Expression::Record { fields: b, .. }) => {
            a.len() == b.len()
                && a.iter()
                    .zip(b)
                    .all(|(a, b)| a.name == b.name && structurally_equal(&a.value, &b.value))
        }
        _ => false,
    }
}

fn arguments_equal(left: &[Expression], right: &[Expression]) -> bool {
    left.len() == right.len()
        && left
            .iter()
            .zip(right)
            .all(|(left, right)| structurally_equal(left, right))
}

/// The diagnostic for a repeated case pattern; a literal text is quoted.
fn duplicate_pattern_message(pattern: &Expression) -> String {
    match pattern {
        Expression::Text { value, .. } => format!("duplicate case pattern `{value}`"),
        _ => "duplicate case pattern".to_owned(),
    }
}
