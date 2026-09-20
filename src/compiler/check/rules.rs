//! The validation walk over one declaration body.
//!
//! Names are already edges, so the walker only reads and writes node kinds.
//! Kinds are derived bottom-up: the walk returns the kind of each expression
//! it visits and records the local binding kinds and the function's return
//! kind. The usage matrix decides where each kind may stand.

use std::collections::HashMap;

use crate::compiler::{
    Accept, BindingKind, DeclarationId, DeclarationKind, Diagnostic, Expression, Field, FileId,
    Kind, Method, Native, Position, Program, Statement, fits, method_row, native_row,
};
use crate::syntax::Span;

/// The walker over one body. Names are already edges, so it only reads and
/// writes node kinds.
pub(super) struct Walk<'a> {
    program: &'a Program,
    file: FileId,
    /// Whether the walked body is lexically inside a function. `std::async`
    /// is only allowed inside a function, so a module initializer cannot
    /// spawn.
    inside_function: bool,
    pub(super) locals: HashMap<DeclarationId, Kind>,
    pub(super) return_kind: Option<Kind>,
}

impl<'a> Walk<'a> {
    pub(super) fn new(program: &'a Program, file: FileId, inside_function: bool) -> Self {
        Self {
            program,
            file,
            inside_function,
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
            .unwrap_or(Kind::Text)
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
                    // native, or run a command chain. A value on its own is
                    // dead text, so the shape is rejected before the kind.
                    if !matches!(
                        expression,
                        Expression::Call { .. } | Expression::Method { .. }
                    ) {
                        return Err(self.error(
                            expression.span(),
                            "a statement must be a call or a method chain",
                        ));
                    }
                    self.require(expression, Position::Statement)?;
                }
                Statement::Return { value, .. } => self.return_value(value)?,
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
            DeclarationKind::Binding(BindingKind::Parameter(_)) => {
                let name = self.program.declaration(declaration).name.clone();
                Err(self.error(
                    name_span,
                    format!("`{name}` is a parameter and is read-only"),
                ))
            }
            DeclarationKind::Binding(BindingKind::Text | BindingKind::Record) => {
                let binding_kind = self.kind_of(declaration);
                let actual = self.expression(value)?;
                if actual == binding_kind {
                    Ok(())
                } else {
                    Err(self.error(
                        value.span(),
                        format!("expected {}, found {}", binding_kind.name(), actual.name()),
                    ))
                }
            }
            _ => {
                let name = self.program.declaration(declaration).name.clone();
                Err(self.error(
                    name_span,
                    format!("`{name}` is a module-level constant and cannot be assigned"),
                ))
            }
        }
    }

    /// Validate a returned expression and record the function's return kind.
    fn return_value(&mut self, value: &Expression) -> Result<(), Diagnostic> {
        let actual = self.require(value, Position::Return)?;
        self.return_kind = Some(actual);
        Ok(())
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
            Expression::Method {
                target,
                method,
                arguments,
                span,
            } => self.method(target, *method, arguments, *span),
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
                format!(
                    "`{}` is a function; call it",
                    self.program.display(declaration)
                ),
            )),
        }
    }

    fn call(
        &mut self,
        callee: DeclarationId,
        callee_span: Span,
        arguments: &[Expression],
    ) -> Result<Kind, Diagnostic> {
        match &self.program.declaration(callee).kind {
            DeclarationKind::Native(native) => {
                let row = native_row(*native);
                self.arity(callee, callee_span, arguments, row.accepts.len())?;
                if *native == Native::Async && !self.inside_function {
                    return Err(self.error(
                        callee_span,
                        "`std::async` is only allowed inside a function",
                    ));
                }
                let display = self.program.display(callee);
                for (argument, accept) in arguments.iter().zip(row.accepts) {
                    self.argument_accept(&display, argument, *accept)?;
                }
                Ok(row.returns)
            }
            DeclarationKind::Function(function) => {
                let parameters = function.parameters.clone();
                self.arity(callee, callee_span, arguments, parameters.len())?;
                for (argument, parameter) in arguments.iter().zip(&parameters) {
                    // A parameter carries the kind its declaration writes, so
                    // every argument is checked against that fixed kind.
                    let expected = self.kind_of(*parameter);
                    let actual = self.expression(argument)?;
                    if actual != expected {
                        return Err(self.error(
                            argument.span(),
                            format!("expected {}, found {}", expected.name(), actual.name()),
                        ));
                    }
                }
                Ok(self
                    .program
                    .declaration(callee)
                    .derived
                    .kind
                    .unwrap_or(Kind::Nothing))
            }
            _ => Err(self.error(
                callee_span,
                format!("`{}` is not a function", self.program.display(callee)),
            )),
        }
    }

    /// Check one argument against a registry row entry.
    fn argument_accept(
        &mut self,
        display: &str,
        argument: &Expression,
        accept: Accept,
    ) -> Result<(), Diagnostic> {
        match accept {
            Accept::Usage(required) => {
                let actual = self.expression(argument)?;
                if fits(actual.usage(), required) {
                    return Ok(());
                }
                Err(self.error(
                    argument.span(),
                    format!("expected {}, found {}", required.name(), actual.name()),
                ))
            }
            Accept::Invocation => self.invocation_argument(display, argument),
        }
    }

    /// `Accept::Invocation`: the argument must be an invocation, a call or a
    /// method chain. Its own arguments are checked here, because a spawned
    /// thread evaluates them at the spawn site.
    fn invocation_argument(
        &mut self,
        display: &str,
        argument: &Expression,
    ) -> Result<(), Diagnostic> {
        if !matches!(
            argument,
            Expression::Call { .. } | Expression::Method { .. }
        ) {
            return Err(self.error(
                argument.span(),
                format!("`{display}` takes a call or a method chain"),
            ));
        }
        self.expression(argument)?;
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

    fn method(
        &mut self,
        target: &Expression,
        method: Method,
        arguments: &[Expression],
        span: Span,
    ) -> Result<Kind, Diagnostic> {
        let row = method_row(method);
        if arguments.len() != row.accepts.len() {
            return Err(self.error(
                span,
                format!(
                    "`.{}` takes {} arguments, found {}",
                    row.name,
                    row.accepts.len(),
                    arguments.len()
                ),
            ));
        }
        self.validate_chain(target, method, span)?;
        self.require(target, Position::ChainTarget)?;
        let display = format!(".{}", row.name);
        for (argument, accept) in arguments.iter().zip(row.accepts) {
            self.argument_accept(&display, argument, *accept)?;
        }
        if method == Method::Timeout
            && let Expression::Text { value, span } = &arguments[0]
            && (value.is_empty() || !value.chars().all(|character| character.is_ascii_digit()))
        {
            return Err(self.error(*span, "a timeout literal must be whole seconds"));
        }
        Ok(row.returns)
    }

    /// A command chain calls each method at most once, and `.out()` and
    /// `.code()` cannot be combined.
    fn validate_chain(
        &self,
        target: &Expression,
        method: Method,
        span: Span,
    ) -> Result<(), Diagnostic> {
        let mut chain = Vec::new();
        collect_chain_methods(target, &mut chain);
        if chain.contains(&method) {
            return Err(self.error(
                span,
                format!("`.{}` is called twice in one command chain", method.name()),
            ));
        }
        if method_row(method).is_terminal()
            && chain.iter().any(|other| method_row(*other).is_terminal())
        {
            return Err(self.error(span, "`.out()` and `.code()` cannot be combined"));
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
        let actual = self.expression(expression)?;
        let required = position.required_usage();
        if fits(actual.usage(), required) {
            return Ok(actual);
        }
        Err(self.error(
            expression.span(),
            format!("expected {}, found {}", required.name(), actual.name()),
        ))
    }
}

/// Collect the methods a command chain applies, innermost first.
fn collect_chain_methods(target: &Expression, found: &mut Vec<Method>) {
    if let Expression::Method { target, method, .. } = target {
        found.push(*method);
        collect_chain_methods(target, found);
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
            Expression::Method {
                target: a,
                method: a_method,
                arguments: a_arguments,
                ..
            },
            Expression::Method {
                target: b,
                method: b_method,
                arguments: b_arguments,
                ..
            },
        ) => {
            a_method == b_method
                && structurally_equal(a, b)
                && arguments_equal(a_arguments, b_arguments)
        }
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
