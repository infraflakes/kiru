//! Checking one declaration body.
//!
//! Names are already edges, so the checker only reads and writes node types.
//! The statement walk and the flow rules live here; the typing of an
//! expression lives in `body_expressions`. A function's return type is
//! declared, so a return is checked against it rather than accumulated, and a
//! call that produces no value may only stand as a statement.

use crate::compiler::{Diagnostic, expected_instead, private_member};
use crate::model::Type;
use crate::model::{
    DeclarationId, Expression, FileId, Function, MatchArm, MatchBody, NamespaceId, Program,
    Statement, TypeId, TypeKind, rec_field_decl, substitute,
};
use crate::syntax::Span;

use super::body_expressions::is_exhaustive;
use super::flow::Flow;

/// The checker over one body. Names are already edges, so it only reads and
/// writes node types.
pub(in crate::compiler::validate_program) struct Checker<'a> {
    pub(super) program: &'a Program,
    file: FileId,
    /// The namespace the body is written in. A field or variant is reached
    /// freely here and from another namespace only when it is `pub`.
    namespace: NamespaceId,
    /// The name of the function being walked. A module value walk has none,
    /// and a `return` cannot occur in one.
    function_name: Option<&'a str>,
    /// The function's declared return type, or `None` when the function
    /// declares none (and for a module value walk, which has no return). A
    /// function with no return type produces no value, so no type names it.
    return_type: Option<Type>,
    /// Whether the walk is inside a `for` body, where `break` is allowed.
    in_loop: bool,
    /// The types of the loop variables and variant-pattern bindings whose
    /// bodies are being walked. Their binding nodes are fixed at link time, so
    /// a substituted type is kept here for the walk.
    pub(super) overrides: Vec<(DeclarationId, Type)>,
    /// The enclosing locals a closure captured by value. They are read-only in
    /// the closure body: an assignment would only change the copy.
    frozen: Vec<DeclarationId>,
}
impl<'a> Checker<'a> {
    /// A walk over a module value initializer. It never visits a statement.
    pub(in crate::compiler::validate_program) fn new(
        program: &'a Program,
        file: FileId,
        namespace: NamespaceId,
    ) -> Self {
        Self {
            program,
            file,
            namespace,
            function_name: None,
            return_type: None,
            in_loop: false,
            overrides: Vec::new(),
            frozen: Vec::new(),
        }
    }

    /// A walk over a function body. A return is checked against the declared
    /// return type, or against no type when the function declares none.
    pub(in crate::compiler::validate_program) fn for_function(
        program: &'a Program,
        file: FileId,
        namespace: NamespaceId,
        function_name: &'a str,
        return_type: Option<Type>,
    ) -> Self {
        Self {
            program,
            file,
            namespace,
            function_name: Some(function_name),
            return_type,
            in_loop: false,
            overrides: Vec::new(),
            frozen: Vec::new(),
        }
    }

    pub(super) fn error(&self, span: Span, message: impl Into<String>) -> Diagnostic {
        Diagnostic::new(&self.program.file(self.file).path, span, message)
    }

    /// The type of a binding or a module value. A `for` item and a variant
    /// pattern binding take the type in `overrides`.
    pub(super) fn kind_of(&self, id: DeclarationId) -> Type {
        if let Some((_, ty)) = self.overrides.iter().rev().find(|(item, _)| *item == id) {
            return ty.clone();
        }
        self.program.declaration(id).ty.clone()
    }

    /// Whether a member declared in `declaring` is reachable from the walk's
    /// namespace: its own namespace always, another namespace only when the
    /// member is `pub`. The one visibility rule, shared by a field and a
    /// variant and matching the rule a name already has.
    fn member_reachable(&self, public: bool, declaring: NamespaceId) -> bool {
        public || declaring == self.namespace
    }

    /// Reject reading, writing, or building a field of `type_id` when the field
    /// is private to another namespace. One gate serves a field read, a field
    /// in a `{ … }` literal, and a field assignment.
    pub(super) fn check_field(
        &self,
        type_id: TypeId,
        name: &str,
        public: bool,
        span: Span,
    ) -> Result<(), Diagnostic> {
        let declaration = self.program.type_decl(type_id);
        if self.member_reachable(public, declaration.namespace) {
            return Ok(());
        }
        let namespace = self.namespace_name(declaration.namespace);
        Err(self.error(span, private_member(&declaration.name, name, &namespace)))
    }

    /// Reject constructing or matching a variant of `type_id` when the variant
    /// is private to another namespace. One gate serves a construction and a
    /// pattern.
    pub(super) fn check_variant(
        &self,
        type_id: TypeId,
        name: &str,
        public: bool,
        span: Span,
    ) -> Result<(), Diagnostic> {
        let declaration = self.program.type_decl(type_id);
        if self.member_reachable(public, declaration.namespace) {
            return Ok(());
        }
        let namespace = self.namespace_name(declaration.namespace);
        Err(self.error(span, private_member(&declaration.name, name, &namespace)))
    }

    /// The namespace path for a diagnostic: the root namespace has no path.
    fn namespace_name(&self, namespace: NamespaceId) -> String {
        let path = self.program.namespace_path(namespace);
        if path.is_empty() {
            "the root namespace".to_owned()
        } else {
            crate::model::join_path(&path, false)
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
                let ty = self.kind_of(*declaration);
                self.require(value, &ty)?;
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
                field,
                value,
                ..
            } => {
                self.field_assign(*declaration, *name_span, field, value)?;
                Ok(Flow::FALLS)
            }
            Statement::IndexAssign {
                declaration,
                name_span,
                index,
                value,
                ..
            } => {
                self.index_assign(*declaration, *name_span, index, value)?;
                Ok(Flow::FALLS)
            }
            Statement::Expression(expression) => match expression {
                // A bare call runs for its effect; a call that halts ends the
                // path.
                Expression::Call { .. } => {
                    self.expression(expression, None)?;
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
                self.expression(call, None)?;
                Ok(Flow::FALLS)
            }
            Statement::Wait { .. } => Ok(Flow::FALLS),
            Statement::ForEach {
                item,
                iterable,
                body,
                ..
            } => {
                let Some(iterable_type) = self.expression(iterable, None)? else {
                    return Err(self.error(iterable.span(), expected_instead("Vec", "no value")));
                };
                let element = match iterable_type {
                    Type::Vec(element) => *element,
                    other => {
                        return Err(self.error(
                            iterable.span(),
                            expected_instead("Vec", &self.type_name(&other)),
                        ));
                    }
                };
                let was_in_loop = self.in_loop;
                self.in_loop = true;
                self.overrides.push((*item, element));
                let body_flow = self.statements(body);
                self.overrides.pop();
                self.in_loop = was_in_loop;
                let body_flow = body_flow?;
                // A Vec `for` may run zero times, so it always falls through; a
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
            && let Expression::Reference { declaration, .. } = callee.as_ref()
            && self.program.declaration(*declaration).derived.halts
        {
            return Flow::STOPS;
        }
        Flow::FALLS
    }

    /// The declared type of an assignment target. Every target is a local
    /// binding; a module value is a constant, and a local a closure captured by
    /// value is frozen, since the closure holds a copy an assignment would not
    /// reach. One target policy serves every assignment form.
    fn assignment_target(
        &self,
        declaration: DeclarationId,
        name_span: Span,
    ) -> Result<Type, Diagnostic> {
        if self.frozen.contains(&declaration) {
            let name = &self.program.declaration(declaration).name;
            return Err(self.error(
                name_span,
                format!("`{name}` is captured by a closure and cannot be assigned"),
            ));
        }
        let node = self.program.declaration(declaration);
        if node.is_binding() {
            return Ok(self.kind_of(declaration));
        }
        Err(self.error(
            name_span,
            format!(
                "`{}` is a module-level constant and cannot be assigned",
                node.name
            ),
        ))
    }
    /// Prove a function body in the context where it is written, so a captured
    /// loop variable or variant-pattern binding keeps the type the enclosing
    /// walk knows (those types live in `overrides`, not in the binding node),
    /// and return its flow. A named function's body is proved here too, with
    /// its declaration name for diagnostics.
    pub(in crate::compiler::validate_program) fn check_function(
        &mut self,
        function: &Function,
        name: &'a str,
    ) -> Result<Flow, Diagnostic> {
        let span = function.span;
        let return_type = function.ty.return_type();
        let saved_return = std::mem::replace(&mut self.return_type, return_type.clone());
        let saved_name = self.function_name.replace(name);
        let saved_loop = std::mem::replace(&mut self.in_loop, false);
        let saved_frozen = std::mem::replace(&mut self.frozen, function.captures.clone());
        let flow = self.statements(&function.body);
        self.return_type = saved_return;
        self.function_name = saved_name;
        self.in_loop = saved_loop;
        self.frozen = saved_frozen;
        let flow = flow?;
        if let Some(return_type) = &return_type
            && flow.falls
        {
            return Err(self.error(
                span,
                format!(
                    "`{name}` is declared to return {}, so every path must end with `return ...`, `panic;`, or a call that stops the run",
                    self.type_name(return_type)
                ),
            ));
        }
        Ok(flow)
    }
    fn assign(
        &mut self,
        declaration: DeclarationId,
        name_span: Span,
        value: &Expression,
    ) -> Result<(), Diagnostic> {
        let ty = self.assignment_target(declaration, name_span)?;
        self.require(value, &ty)?;
        Ok(())
    }

    /// Assign one field of a binding. The target must be a declared rec type;
    /// the value must fit the field's type.
    fn field_assign(
        &mut self,
        declaration: DeclarationId,
        name_span: Span,
        field: &str,
        value: &Expression,
    ) -> Result<(), Diagnostic> {
        let name = self.program.declaration(declaration).name.clone();
        match self.assignment_target(declaration, name_span)? {
            Type::Named(id, arguments) => {
                match &self.program.type_decl(id).kind {
                    TypeKind::Rec(_) => {}
                    TypeKind::Enum(_) => {
                        return Err(self.error(name_span, format!("`{name}` is an enum; match it")));
                    }
                }
                match rec_field_decl(&self.program.types, id, field) {
                    Some(field_decl) => {
                        self.check_field(id, field, field_decl.public, name_span)?;
                        let parameters = &self.program.type_decl(id).parameters;
                        let ty = substitute(&field_decl.ty, parameters, &arguments);
                        self.require(value, &ty)?;
                        Ok(())
                    }
                    None => Err(self.error(name_span, format!("`{name}` has no field `{field}`"))),
                }
            }
            _ => Err(self.error(
                name_span,
                format!("`{name}` is not a rec and has no fields"),
            )),
        }
    }

    /// Enforce an index assignment `name[index] = value;`: the target must be a
    /// Vec, the index an `int`, and the value of the element type.
    fn index_assign(
        &mut self,
        declaration: DeclarationId,
        name_span: Span,
        index: &Expression,
        value: &Expression,
    ) -> Result<(), Diagnostic> {
        let name = self.program.declaration(declaration).name.clone();
        let Type::Vec(element) = self.assignment_target(declaration, name_span)? else {
            return Err(self.error(name_span, format!("`{name}` is not a Vec")));
        };
        self.require(index, &Type::Int)?;
        self.require(value, &element)?;
        Ok(())
    }

    /// Enforce the return rules of the enclosing function. A value return is
    /// only for a function declared with a return type, and its value must fit
    /// that type; a bare return is only for a function declared with no return
    /// type.
    fn return_statement(
        &mut self,
        value: Option<&Expression>,
        span: Span,
    ) -> Result<(), Diagnostic> {
        let Some(name) = self.function_name else {
            return Err(self.error(span, "`return` is not allowed here"));
        };
        let return_type = self.return_type.clone();
        if let Some(value) = value {
            if matches!(value, Expression::Match { .. }) {
                return Err(self.error(
                    value.span(),
                    "`return` does not take a `match`; bind it first: `let name = match ...; return name;`",
                ));
            }
            let Some(return_type) = return_type else {
                return Err(self.error(
                    value.span(),
                    format!(
                        "`{name}` is declared to return nothing, so `return` cannot carry a value"
                    ),
                ));
            };
            self.require(value, &return_type)?;
            return Ok(());
        }
        match return_type {
            Some(return_type) => Err(self.error(
                span,
                format!(
                    "`{name}` is declared to return {}, so `return` must carry a value",
                    self.type_name(&return_type)
                ),
            )),
            None => Ok(()),
        }
    }
    /// The name of a type for a diagnostic.
    pub(super) fn type_name(&self, ty: &Type) -> String {
        self.program.type_name(ty)
    }
    /// The flow of a match in statement position: the union of the taken arm's
    /// flow, plus a fall-through when the match is not exhaustive.
    fn match_statement(
        &mut self,
        subject: &Expression,
        cases: &[MatchArm],
        default: &Option<MatchBody>,
    ) -> Result<Flow, Diagnostic> {
        let subject_type = self.match_subject(subject)?;
        let mut seen: Vec<&Expression> = Vec::new();
        let mut covered = self.enum_coverage(&subject_type);
        let mut flow = Flow::STOPS;
        for arm in cases {
            let pushed =
                self.match_pattern(&arm.pattern, &subject_type, &mut seen, covered.as_mut())?;
            flow = flow.union(self.match_body_flow(&arm.body)?);
            for _ in 0..pushed {
                self.overrides.pop();
            }
        }
        match default {
            Some(default) => flow = flow.union(self.match_body_flow(default)?),
            None if is_exhaustive(default, &subject_type, cases, covered.as_deref()) => {}
            None => flow = flow.union(Flow::FALLS),
        }
        Ok(flow)
    }
    /// The flow of a match arm's body.
    fn match_body_flow(&mut self, body: &MatchBody) -> Result<Flow, Diagnostic> {
        match body {
            MatchBody::Expression(expression) => {
                self.expression(expression, None)?;
                Ok(Flow::FALLS)
            }
            MatchBody::Block(statements) => self.statements(statements),
        }
    }
}
