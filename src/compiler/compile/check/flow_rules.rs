//! The flow pass: how every body ends.
//!
//! Typing does not depend on whether a callee stops the run, and this pass
//! needs no type but a function's declared return. It walks statements twice:
//! a fixpoint that settles every function's `halts` fact, then one report walk
//! that reads the settled facts and reports the fall-through, break-placement
//! and return-placement diagnostics directly. No diagnostic is buffered across
//! passes, and one body's placement error never stops another body's walk.

use crate::compiler::Diagnostic;
use crate::compiler::model::{
    DeclarationId, Expression, FileId, Function, MatchBody, ModelView, Program, Statement,
};
use crate::syntax::Span;

use super::flow::Flow;

/// Settle every function's halting fact, then report every flow diagnostic.
pub(super) fn check_flow(program: &mut Program) -> Vec<Diagnostic> {
    settle_halts(program);
    report_flow(program)
}

/// Raise every function's `halts` fact until a whole pass changes nothing. The
/// walk reads the facts of the pass before it, so a forward call and a cycle
/// settle the same way a reachability walk does. A body whose placement rules
/// fail has no settled flow; its error is reported by the report walk.
fn settle_halts(program: &mut Program) {
    let mut model = ModelView::from(program);
    loop {
        let mut changed = false;
        for index in 0..model.declarations().len() {
            let id = DeclarationId(index);
            let kind = model.declaration(id).kind;
            if !(kind.callable() && kind.has_body()) {
                continue;
            }
            let Ok(flow) = body_flow(&model, id, false) else {
                continue;
            };
            let halts = !flow.falls && !flow.returns;
            if model.declaration(id).derived.halts != halts {
                model.set_halts(id, halts);
                changed = true;
            }
        }
        if !changed {
            return;
        }
    }
}

/// Walk every body once against the settled halting facts and report every
/// flow diagnostic.
fn report_flow(program: &mut Program) -> Vec<Diagnostic> {
    let model = ModelView::from(program);
    let mut diagnostics = Vec::new();
    for index in 0..model.declarations().len() {
        let id = DeclarationId(index);
        let kind = model.declaration(id).kind;
        if kind.callable() && kind.has_body() {
            let walk = FlowWalk::for_declaration(&model, id, true);
            let function = model
                .declaration(id)
                .function()
                .expect("a callable body is a function expression");
            match walk.statements(&function.body, false) {
                Err(diagnostic) => diagnostics.push(diagnostic),
                Ok(flow) if walk.return_type.is_some() && flow.falls => {
                    diagnostics.push(walk.fall_through(function.span));
                }
                Ok(_) => {}
            }
        } else if kind.module_value() {
            let declaration = model.declaration(id);
            let file = declaration
                .file
                .expect("a module value is a file declaration, so it has a file");
            let walk = FlowWalk {
                model: &model,
                file,
                return_type: None,
                name: BodyName::ModuleValue,
                report: true,
            };
            let value = declaration
                .value()
                .expect("a module value carries its initializer");
            if let Err(diagnostic) = walk.expression(value, false) {
                diagnostics.push(diagnostic);
            }
        }
    }
    diagnostics
}

/// The flow of one declaration's function body, with diagnostics suppressed or
/// reported according to `report`.
fn body_flow(model: &ModelView<'_>, id: DeclarationId, report: bool) -> Result<Flow, Diagnostic> {
    let walk = FlowWalk::for_declaration(model, id, report);
    let function = model
        .declaration(id)
        .function()
        .expect("a callable body is a function expression");
    walk.statements(&function.body, false)
}

/// The name a body reports under in a diagnostic.
#[derive(Clone, Copy)]
enum BodyName {
    Declaration(DeclarationId),
    Closure,
    ModuleValue,
}

/// One body's flow walk. It reports placement diagnostics always; the
/// fall-through diagnostic only after the halting facts have settled.
struct FlowWalk<'m, 'v> {
    model: &'m ModelView<'v>,
    file: FileId,
    /// The body's declared return type, when it declares one.
    return_type: Option<crate::compiler::model::Type>,
    name: BodyName,
    report: bool,
}

impl<'m, 'v> FlowWalk<'m, 'v> {
    fn for_declaration(model: &'m ModelView<'v>, id: DeclarationId, report: bool) -> Self {
        let declaration = model.declaration(id);
        Self {
            model,
            file: declaration
                .file
                .expect("a body declaration is a file declaration"),
            return_type: declaration.return_type(model.types()),
            name: BodyName::Declaration(id),
            report,
        }
    }

    fn error(&self, span: Span, message: impl Into<String>) -> Diagnostic {
        Diagnostic::new(&self.model.file(self.file).path, span, message)
    }

    /// The body's name for a diagnostic.
    fn name(&self) -> &str {
        match self.name {
            BodyName::Declaration(id) => &self.model.declaration(id).name,
            BodyName::Closure => "<closure>",
            BodyName::ModuleValue => "",
        }
    }

    fn fall_through(&self, span: Span) -> Diagnostic {
        let return_type = self
            .return_type
            .as_ref()
            .expect("only a body with a return type can fall through");
        self.error(
            span,
            format!(
                "`{}` is declared to return {}, so every path must end with `return ...`, `panic;`, or a call that stops the run",
                self.name(),
                self.model.type_name(return_type)
            ),
        )
    }

    /// A sequence of statements: every statement is walked, and only the
    /// statements a falling path reaches contribute to the flow.
    fn statements(&self, statements: &[Statement], in_loop: bool) -> Result<Flow, Diagnostic> {
        let mut flow = Flow::FALLS;
        for statement in statements {
            let next = self.statement(statement, in_loop)?;
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

    fn statement(&self, statement: &Statement, in_loop: bool) -> Result<Flow, Diagnostic> {
        match statement {
            Statement::Bind { value, .. } => {
                self.expression(value, in_loop)?;
                Ok(Flow::FALLS)
            }
            Statement::Assign { value, .. } | Statement::FieldAssign { value, .. } => {
                self.expression(value, in_loop)?;
                Ok(Flow::FALLS)
            }
            Statement::Expression(expression) => match expression {
                // A bare call runs for its effect; a call that halts ends the
                // path.
                Expression::Call { .. } => {
                    self.expression(expression, in_loop)?;
                    Ok(self.call_flow(expression))
                }
                // A bare match runs its taken arm; its flow is the arms' union,
                // plus a fall-through when the arms do not cover every value.
                Expression::Match {
                    subject,
                    cases,
                    default,
                    exhaustive,
                    ..
                } => {
                    self.expression(subject, in_loop)?;
                    let mut flow = Flow::STOPS;
                    for arm in cases {
                        flow = flow.union(self.match_body(&arm.body, in_loop)?);
                    }
                    if let Some(default) = default {
                        flow = flow.union(self.match_body(default, in_loop)?);
                    }
                    if !exhaustive {
                        flow = flow.union(Flow::FALLS);
                    }
                    Ok(flow)
                }
                _ => {
                    self.expression(expression, in_loop)?;
                    Ok(Flow::FALLS)
                }
            },
            Statement::Return { value, span } => {
                self.return_statement(value.as_ref(), *span, in_loop)?;
                Ok(Flow::RETURNS)
            }
            Statement::Panic { .. } => Ok(Flow::STOPS),
            Statement::Async { call, .. } => {
                self.expression(call, in_loop)?;
                Ok(Flow::FALLS)
            }
            Statement::Wait { .. } => Ok(Flow::FALLS),
            Statement::Forever { body, .. } => {
                let body_flow = self.statements(body, true)?;
                // An unbounded `for` falls through only when its body can
                // break; a returning path in the body still returns.
                Ok(Flow {
                    returns: body_flow.returns,
                    falls: body_flow.breaks,
                    breaks: false,
                })
            }
            Statement::Break { span } => {
                if !in_loop {
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
            && let Expression::Reference { declaration, .. } = callee.as_ref()
            && self.model.declaration(*declaration).derived.halts
        {
            return Flow::STOPS;
        }
        Flow::FALLS
    }

    /// Enforce the return placement rules of the enclosing body. A value return
    /// is only for a function declared with a return type, and a bare return is
    /// only for one declared with none; a module value initializer has no
    /// return at all.
    fn return_statement(
        &self,
        value: Option<&Expression>,
        span: Span,
        in_loop: bool,
    ) -> Result<(), Diagnostic> {
        if let Some(value) = value {
            self.expression(value, in_loop)?;
        }
        if let BodyName::ModuleValue = self.name {
            return Err(self.error(span, "`return` is not allowed here"));
        }
        match (value, &self.return_type) {
            (Some(value), None) => Err(self.error(
                value.span(),
                format!(
                    "`{}` is declared to return nothing, so `return` cannot carry a value",
                    self.name()
                ),
            )),
            (None, Some(return_type)) => Err(self.error(
                span,
                format!(
                    "`{}` is declared to return {}, so `return` must carry a value",
                    self.name(),
                    self.model.type_name(return_type)
                ),
            )),
            _ => Ok(()),
        }
    }

    /// Walk one expression for the bodies and placements it contains. A
    /// closure's body is a body of its own; a match arm's block is walked with
    /// the enclosing loop, so a `break` there leaves that loop.
    fn expression(&self, expression: &Expression, in_loop: bool) -> Result<(), Diagnostic> {
        match expression {
            Expression::Text { .. }
            | Expression::Bool { .. }
            | Expression::Int { .. }
            | Expression::Reference { .. }
            | Expression::Method { .. } => Ok(()),
            Expression::Map { fields, .. } => {
                for field in fields {
                    self.expression(&field.value, in_loop)?;
                }
                Ok(())
            }
            Expression::Interpolated { parts, .. } => {
                for part in parts {
                    if let crate::compiler::model::StringPart::Expression(expression) = part {
                        self.expression(expression, in_loop)?;
                    }
                }
                Ok(())
            }
            Expression::Variant { payload, .. } => {
                for element in payload {
                    self.expression(element, in_loop)?;
                }
                Ok(())
            }
            Expression::Call {
                callee, arguments, ..
            } => {
                self.expression(callee, in_loop)?;
                for argument in arguments {
                    self.expression(argument, in_loop)?;
                }
                Ok(())
            }
            Expression::Field { target, .. } => self.expression(target, in_loop),
            Expression::Binary { left, right, .. } => {
                self.expression(left, in_loop)?;
                self.expression(right, in_loop)
            }
            Expression::Unary { operand, .. } => self.expression(operand, in_loop),
            Expression::Match {
                subject,
                cases,
                default,
                ..
            } => {
                self.expression(subject, in_loop)?;
                for arm in cases {
                    self.match_body(&arm.body, in_loop)?;
                }
                if let Some(default) = default {
                    self.match_body(default, in_loop)?;
                }
                Ok(())
            }
            Expression::Function(function) => self.function_body(function),
        }
    }

    /// Walk a closure's body as a function of its own: a `break` or `return`
    /// inside it belongs to it, not to the enclosing body.
    fn function_body(&self, function: &Function) -> Result<(), Diagnostic> {
        let inner = FlowWalk {
            model: self.model,
            file: self.file,
            return_type: function.declared_return.clone(),
            name: BodyName::Closure,
            report: self.report,
        };
        let flow = inner.statements(&function.body, false)?;
        if inner.report && inner.return_type.is_some() && flow.falls {
            return Err(inner.fall_through(function.span));
        }
        Ok(())
    }

    fn match_body(&self, body: &MatchBody, in_loop: bool) -> Result<Flow, Diagnostic> {
        match body {
            MatchBody::Expression(expression) => {
                self.expression(expression, in_loop)?;
                Ok(Flow::FALLS)
            }
            MatchBody::Block(statements) => self.statements(statements, in_loop),
        }
    }
}
