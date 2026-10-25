//! Checking one declaration body.
//!
//! Names are already edges, so the checker only reads the immutable graph and
//! records what it derives in `Facts`. The statement walk and the typing rules
//! live here; the typing of an expression lives in `expression_typing`, and
//! the match rules live in `match_rules`. A function's return type is
//! declared, so a return is checked against it rather than accumulated, and a
//! call that produces no value may only stand as a statement.
//!
//! This pass only types. Whether a body falls through, breaks out of a loop, or
//! returns where it may not is a fact about the statement walk alone, so the
//! flow pass reads the graph after this one. The facts it records here are the
//! ones a later pass reads: a pattern binding's instantiated type, a method
//! call's target, and a match's coverage.

use crate::compiler::model::Type;
use crate::compiler::model::{
    Binding, DeclarationId, Expression, Facts, FileId, Function, MatchArm, MatchBody, ModelView,
    NamespaceId, Site, Statement, reachable, rec_field, substitute,
};
use crate::compiler::{Diagnostic, private_member};
use crate::syntax::Span;

use super::match_rules::is_exhaustive;

/// The checker over one body. Names are already edges, so it only reads the
/// graph and records the facts it derives.
pub(in crate::compiler::compile::check) struct Checker<'a> {
    pub(super) model: ModelView<'a>,
    /// The derived store beside the graph. The checker writes here, never to
    /// a declaration or an expression.
    pub(super) facts: &'a mut Facts,
    /// The declaration whose body this walk checks. A fact about an expression
    /// site is keyed by this owner and the expression's span.
    pub(super) owner: DeclarationId,
    pub(super) file: FileId,
    /// The namespace the body is written in. A field or variant is reached
    /// freely here and from another namespace only when it is `pub`.
    pub(super) namespace: NamespaceId,
    /// The function's declared return type, or `None` when the function
    /// declares none (and for a module value walk, which has no return).
    pub(super) return_type: Option<Type>,
    /// The enclosing locals a closure captured by value. They are read-only in
    /// the closure body: an assignment would only change the copy.
    frozen: Vec<DeclarationId>,
}
impl<'a> Checker<'a> {
    /// A walk over a body. A module value initializer sets no return type; a
    /// function sets its declaration's.
    pub(in crate::compiler::compile::check) fn new(
        model: ModelView<'a>,
        facts: &'a mut Facts,
        owner: DeclarationId,
        file: FileId,
        namespace: NamespaceId,
    ) -> Self {
        Self {
            model,
            facts,
            owner,
            file,
            namespace,
            return_type: None,
            frozen: Vec::new(),
        }
    }

    pub(super) fn error(&self, span: Span, message: impl Into<String>) -> Diagnostic {
        Diagnostic::new(&self.model.file(self.file).path, span, message)
    }

    /// The type a binding holds at a use site: the type its arm instantiated
    /// when it is a variant-pattern binding, and its declared type otherwise.
    pub(super) fn kind_of(&self, id: DeclarationId) -> Type {
        self.facts
            .pattern_type(id)
            .cloned()
            .unwrap_or_else(|| self.model.declaration(id).ty.clone())
    }

    /// Whether a member declared in `declaring` is reachable from the walk's
    /// namespace: its own namespace always, another namespace only when the
    /// member is `pub`. The one visibility rule, shared by a field and a
    /// variant and matching the rule a name already has.
    fn member_reachable(&self, public: bool, declaring: NamespaceId) -> bool {
        reachable(public, declaring, self.namespace)
    }

    /// Reject reaching a member of `type_id` when the member is private to
    /// another namespace. One gate serves a field read, a field in a `{ … }`
    /// literal, a field assignment, a variant construction, and a variant
    /// pattern.
    pub(super) fn check_member(
        &self,
        type_id: DeclarationId,
        name: &str,
        public: bool,
        span: Span,
    ) -> Result<(), Diagnostic> {
        let declaration = self.model.declaration(type_id);
        if self.member_reachable(public, declaration.namespace) {
            return Ok(());
        }
        let namespace = self.namespace_name(declaration.namespace);
        Err(self.error(span, private_member(&declaration.name, name, &namespace)))
    }

    /// The namespace path for a diagnostic: the root namespace has no path.
    fn namespace_name(&self, namespace: NamespaceId) -> String {
        let path = self.model.namespace_path(namespace);
        if path.is_empty() {
            "the root namespace".to_owned()
        } else {
            crate::compiler::model::join_path(&path)
        }
    }

    /// Type a sequence of statements. Every statement is typed, including the
    /// ones after a terminator; whether a statement is reachable is a flow
    /// fact, and the flow pass reads all of them too.
    pub(in crate::compiler::compile::check) fn statements(
        &mut self,
        statements: &[Statement],
    ) -> Result<(), Diagnostic> {
        for statement in statements {
            self.statement(statement)?;
        }
        Ok(())
    }

    /// Type one statement.
    fn statement(&mut self, statement: &Statement) -> Result<(), Diagnostic> {
        match statement {
            Statement::Bind {
                declaration, value, ..
            } => {
                let ty = self.kind_of(*declaration);
                self.require(value, &ty)?;
                Ok(())
            }
            Statement::Assign {
                declaration,
                name_span,
                value,
                ..
            } => self.assign(*declaration, *name_span, value),
            Statement::FieldAssign {
                declaration,
                name_span,
                field,
                value,
                ..
            } => self.field_assign(*declaration, *name_span, field, value),
            // A bare call runs for its effect; a bare match runs its taken
            // arm. Any other bare value is dead, so the shape is rejected.
            Statement::Expression(expression) => {
                if let Expression::Match {
                    subject,
                    cases,
                    default,
                    span,
                    ..
                } = expression
                {
                    return self.match_statement(subject, cases, default, *span);
                }
                if !matches!(expression, Expression::Call { .. }) {
                    return Err(self.error(expression.span(), "a statement must be a call"));
                }
                self.expression(expression, None)?;
                Ok(())
            }
            Statement::Return { value, .. } => self.return_statement(value.as_ref()),
            Statement::Panic { .. } => Ok(()),
            Statement::Async { call, .. } => {
                if !matches!(call, Expression::Call { .. }) {
                    return Err(self.error(call.span(), "`async` takes a call"));
                }
                self.expression(call, None)?;
                Ok(())
            }
            Statement::Wait { .. } => Ok(()),
            Statement::Forever { body, .. } => self.statements(body),
            // Whether a `break` has a loop to leave is a flow fact.
            Statement::Break { .. } => Ok(()),
        }
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
            let name = &self.model.declaration(declaration).name;
            return Err(self.error(
                name_span,
                format!("`{name}` is captured by a closure and cannot be assigned"),
            ));
        }
        let node = self.model.declaration(declaration);
        if !node.kind.module_value() {
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

    /// Type a function body in the context where it is written, so a captured
    /// loop variable or variant-pattern binding keeps the type the enclosing
    /// walk knows, and so a nested literal's own annotation applies. A named
    /// function's return type is its declaration's; a literal's is its own.
    pub(in crate::compiler::compile::check) fn check_function(
        &mut self,
        function: &Function,
        return_type: Option<Type>,
    ) -> Result<(), Diagnostic> {
        let saved_return = std::mem::replace(&mut self.return_type, return_type);
        let saved_frozen = std::mem::replace(&mut self.frozen, function.captures.clone());
        let result = self.statements(&function.body);
        self.return_type = saved_return;
        self.frozen = saved_frozen;
        result
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
        let name = self.model.declaration(declaration).name.clone();
        match self.assignment_target(declaration, name_span)? {
            Type::Named(id, arguments) => {
                let body = self.model.type_decl(id);
                if body.group().has_variants() {
                    return Err(self.error(name_span, format!("`{name}` is an enum; match it")));
                }
                let Some(field_decl) = rec_field(self.model.declarations(), id, field) else {
                    return Err(self.error(name_span, format!("`{name}` has no field `{field}`")));
                };
                self.check_member(id, field, field_decl.public, name_span)?;
                let ty = substitute(
                    &field_decl.ty,
                    self.model.declarations(),
                    &body.parameters,
                    Binding::Complete(&arguments),
                );
                self.require(value, &ty)?;
                Ok(())
            }
            _ => Err(self.error(
                name_span,
                format!("`{name}` is not a rec and has no fields"),
            )),
        }
    }

    /// Type the value of a `return`, when the enclosing function declares a
    /// return type. Whether the return is allowed at all is a flow fact.
    fn return_statement(&mut self, value: Option<&Expression>) -> Result<(), Diagnostic> {
        if let Some(value) = value
            && let Some(return_type) = self.return_type.clone()
        {
            self.require(value, &return_type)?;
        }
        Ok(())
    }

    /// The name of a type for a diagnostic.
    pub(super) fn type_name(&self, ty: &Type) -> String {
        self.model.type_name(ty)
    }

    /// Type a match in statement position and record whether its arms cover
    /// every value. The flow pass reads that fact to know whether the match can
    /// fall through.
    fn match_statement(
        &mut self,
        subject: &Expression,
        cases: &[MatchArm],
        default: &Option<MatchBody>,
        span: Span,
    ) -> Result<(), Diagnostic> {
        let subject_type = self.match_subject(subject)?;
        let mut seen: Vec<&Expression> = Vec::new();
        let mut coverage = self.coverage(&subject_type);
        for arm in cases {
            self.match_pattern(&arm.pattern, &subject_type, &mut seen, coverage.as_mut())?;
            self.match_body(&arm.body)?;
        }
        if let Some(default) = default {
            self.match_body(default)?;
        }
        self.facts.set_match_exhaustive(
            Site::new(self.owner, span),
            is_exhaustive(default, coverage.as_ref()),
        );
        Ok(())
    }

    /// Type one match arm's body.
    pub(super) fn match_body(&mut self, body: &MatchBody) -> Result<(), Diagnostic> {
        match body {
            MatchBody::Expression(expression) => {
                self.expression(expression, None)?;
                Ok(())
            }
            MatchBody::Block(statements) => self.statements(statements),
        }
    }
}
