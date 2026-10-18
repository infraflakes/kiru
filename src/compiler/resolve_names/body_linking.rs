//! Linking parsed bodies into model nodes.
//!
//! The linking pass walks each pending declaration's parsed body, creating
//! local bindings and resolving references and calls into edges to declaration
//! nodes. After linking, no phase resolves a name again.

use crate::compiler::Diagnostic;
use crate::compiler::load_files::LoadedProgram;
use crate::compiler::model::Type;
use crate::compiler::model::{
    Declaration, DeclarationId, Derived, Expression, Field, FileId, Function, MatchArm, MatchBody,
    NamespaceId, Program, Statement, StringPart,
};
use crate::syntax::Span;
use crate::syntax::{Expression as ParsedExpression, Statement as ParsedStatement, TypeExpr};

use super::builder::Builder;
use super::scopes::{Context, Scopes};

/// Build the program model from the joined item stream.
pub(crate) fn resolve_names(loaded: &LoadedProgram) -> Result<Program, Diagnostic> {
    let mut builder = Builder::new();
    // Reserve the declared types first, so a native can name the absence type,
    // then register the natives before the declarations that call them.
    builder.reserve_items(loaded)?;
    builder.register_builtins();
    builder.register_declarations(loaded)?;
    builder.check_uses(loaded)?;
    let entry = builder.resolve_entry(loaded)?;
    builder.link_declarations(loaded)?;
    let module_values = builder.order_module_values()?;
    let program = builder.finish(entry, module_values);
    crate::compiler::model::verify_program(&program);
    Ok(program)
}

impl Builder {
    /// Link every declaration in two passes. The first pass settles every
    /// declaration's type, so a body in the second pass can read any callee's
    /// signature whatever the declaration order: resolution is order-free.
    fn link_declarations(&mut self, loaded: &LoadedProgram) -> Result<(), Diagnostic> {
        let pending: Vec<(DeclarationId, FileId, usize, NamespaceId)> = self
            .pending
            .iter()
            .map(|pending| {
                (
                    pending.declaration,
                    pending.file,
                    pending.syntax_index,
                    pending.namespace,
                )
            })
            .collect();
        for &(declaration_id, file, syntax_index, namespace) in &pending {
            let skeleton = &self.skeletons[declaration_id.0];
            let name = skeleton.name.clone();
            let declaration = &loaded.items[syntax_index].declaration;
            let ty = if declaration.kind.callable() {
                let function = declaration
                    .function()
                    .expect("a callable declaration carries a function body");
                self.function_signature(function, namespace, file)?
            } else if declaration.kind.module_value() {
                let binding = declaration
                    .binding()
                    .expect("a module value carries its initializer");
                self.resolve_type(&binding.ty, namespace, file, &[])?
            } else {
                unreachable!("only a callable or a module value is pending linking")
            };
            self.declarations[declaration_id.0] = Some(Declaration {
                kind: declaration.kind,
                name,
                file: Some(file),
                namespace,
                owner: None,
                ty,
                value: None,
                native: None,
                derived: Derived { halts: false },
            });
        }
        for pending in std::mem::take(&mut self.pending) {
            let file = pending.file;
            let namespace = pending.namespace;
            let declaration = &loaded.items[pending.syntax_index].declaration;
            let value = if declaration.kind.callable() {
                let function = declaration
                    .function()
                    .expect("a callable declaration carries a function body");
                let mut scopes = Scopes::default();
                scopes.push();
                let type_parameters = function.type_parameters.clone();
                let mut context = Context {
                    scopes: &mut scopes,
                    namespace,
                    file,
                    current: pending.declaration,
                    type_parameters: &type_parameters,
                };
                let mut parameters = Vec::new();
                for parameter in &function.parameters {
                    let ty = self.resolve_type(&parameter.ty, namespace, file, &type_parameters)?;
                    let id = self.declare_local(
                        context.scopes,
                        LocalDeclaration {
                            file,
                            namespace,
                            name: &parameter.name,
                            name_span: parameter.span,
                            owner: pending.declaration,
                            ty,
                        },
                    )?;
                    context.scopes.declare(&parameter.name, id);
                    parameters.push(id);
                }
                let body = self.link_statements(&function.body, &mut context)?;
                let ty = self.declarations[pending.declaration.0]
                    .as_ref()
                    .expect("the first pass settled every declaration's type")
                    .ty
                    .clone();
                Expression::Function(Function {
                    ty,
                    parameters,
                    body,
                    type_parameters,
                    captures: Vec::new(),
                    span: function.span,
                })
            } else if declaration.kind.module_value() {
                let binding = declaration
                    .binding()
                    .expect("a module value carries its initializer");
                let mut scopes = Scopes::default();
                let mut context = Context {
                    scopes: &mut scopes,
                    namespace,
                    file,
                    current: pending.declaration,
                    type_parameters: &[],
                };
                self.link_expression(&binding.value, &mut context)?
            } else {
                unreachable!("only a callable or a module value is pending linking")
            };
            self.declarations[pending.declaration.0]
                .as_mut()
                .expect("the first pass settled every declaration")
                .value = Some(value);
        }
        Ok(())
    }

    /// Resolve a function's declared type: its parameter types and its return
    /// type. No return type means the function's calls produce no value, so the
    /// function type has no return type.
    fn function_signature(
        &self,
        function: &crate::syntax::Function,
        namespace: NamespaceId,
        file: FileId,
    ) -> Result<Type, Diagnostic> {
        let mut parameters = Vec::new();
        for parameter in &function.parameters {
            parameters.push(self.resolve_type(
                &parameter.ty,
                namespace,
                file,
                &function.type_parameters,
            )?);
        }
        let returns = match &function.return_type {
            Some(expr) => Some(Box::new(self.resolve_type(
                expr,
                namespace,
                file,
                &function.type_parameters,
            )?)),
            None => None,
        };
        Ok(Type::Function {
            parameters,
            returns,
        })
    }

    /// Resolve a written type in a declaration's namespace and file.
    fn resolve_type(
        &self,
        expr: &TypeExpr,
        namespace: NamespaceId,
        file: FileId,
        parameters: &[String],
    ) -> Result<Type, Diagnostic> {
        self.resolve_type_expr(expr, namespace, parameters, &self.files[file.0].path)
    }
    fn link_statements(
        &mut self,
        statements: &[ParsedStatement],
        context: &mut Context,
    ) -> Result<Vec<Statement>, Diagnostic> {
        let mut linked = Vec::new();
        for statement in statements {
            match statement {
                ParsedStatement::Binding(binding) => {
                    let value = self.link_expression(&binding.value, context)?;
                    let declaration = self.declare_local(
                        context.scopes,
                        LocalDeclaration {
                            file: context.file,
                            namespace: context.namespace,
                            name: &binding.name,
                            name_span: binding.name_span,
                            owner: context.current,
                            ty: self.resolve_type(
                                &binding.ty,
                                context.namespace,
                                context.file,
                                context.type_parameters,
                            )?,
                        },
                    )?;
                    context.scopes.declare(&binding.name, declaration);
                    linked.push(Statement::Bind {
                        declaration,
                        value,
                        span: binding.span,
                    });
                }
                ParsedStatement::Assignment {
                    name,
                    name_span,
                    value,
                    span,
                } => {
                    let declaration =
                        self.resolve(context, std::slice::from_ref(name), *name_span)?;
                    let value = self.link_expression(value, context)?;
                    linked.push(Statement::Assign {
                        declaration,
                        name_span: *name_span,
                        value,
                        span: *span,
                    });
                }
                ParsedStatement::FieldAssignment {
                    name,
                    name_span,
                    field,
                    value,
                    span,
                    ..
                } => {
                    let declaration =
                        self.resolve(context, std::slice::from_ref(name), *name_span)?;
                    let value = self.link_expression(value, context)?;
                    linked.push(Statement::FieldAssign {
                        declaration,
                        name_span: *name_span,
                        field: field.clone(),
                        value,
                        span: *span,
                    });
                }
                ParsedStatement::Expression(expression) => {
                    linked.push(Statement::Expression(
                        self.link_expression(expression, context)?,
                    ));
                }
                ParsedStatement::Return { value, span } => {
                    let value = match value {
                        Some(value) => Some(self.link_expression(value, context)?),
                        None => None,
                    };
                    linked.push(Statement::Return { value, span: *span });
                }
                ParsedStatement::Panic { span } => {
                    linked.push(Statement::Panic { span: *span });
                }
                ParsedStatement::Async { call, span } => {
                    linked.push(Statement::Async {
                        call: self.link_expression(call, context)?,
                        span: *span,
                    });
                }
                ParsedStatement::Wait { span } => {
                    linked.push(Statement::Wait { span: *span });
                }
                ParsedStatement::Forever { body, span } => {
                    context.scopes.push();
                    let body = self.link_statements(body, context)?;
                    context.scopes.pop();
                    linked.push(Statement::Forever { body, span: *span });
                }
                ParsedStatement::Break { span } => {
                    linked.push(Statement::Break { span: *span });
                }
            }
        }
        Ok(linked)
    }

    fn link_fields(
        &mut self,
        fields: &[crate::syntax::Field],
        context: &mut Context,
    ) -> Result<Vec<Field>, Diagnostic> {
        let mut linked = Vec::new();
        for field in fields {
            linked.push(Field {
                name: field.name.clone(),
                name_span: field.name_span,
                value: self.link_expression(&field.value, context)?,
                span: field.span,
            });
        }
        Ok(linked)
    }

    pub(super) fn link_expression(
        &mut self,
        expression: &ParsedExpression,
        context: &mut Context,
    ) -> Result<Expression, Diagnostic> {
        Ok(match expression {
            ParsedExpression::Text { value, span } => Expression::Text {
                value: value.clone(),
                span: *span,
            },
            ParsedExpression::Map { fields, span } => Expression::Map {
                fields: self.link_fields(fields, context)?,
                span: *span,
            },
            ParsedExpression::Interpolated { parts, span } => {
                let mut linked_parts = Vec::new();
                for part in parts {
                    linked_parts.push(match part {
                        crate::syntax::StringPart::Literal(value) => {
                            StringPart::Literal(value.clone())
                        }
                        crate::syntax::StringPart::Expression(expression) => {
                            StringPart::Expression(self.link_expression(expression, context)?)
                        }
                    });
                }
                Expression::Interpolated {
                    parts: linked_parts,
                    span: *span,
                }
            }
            ParsedExpression::Match {
                subject,
                cases,
                default,
                span,
            } => {
                let subject = self.link_expression(subject, context)?;
                let mut linked_cases = Vec::new();
                for case in cases {
                    context.scopes.push();
                    let pattern = self.link_pattern(&case.pattern, context)?;
                    let body = self.link_match_body(&case.body, context)?;
                    context.scopes.pop();
                    linked_cases.push(MatchArm {
                        pattern,
                        body,
                        span: case.span,
                    });
                }
                let default = match default {
                    Some(body) => {
                        context.scopes.push();
                        let linked_body = self.link_match_body(body, context)?;
                        context.scopes.pop();
                        Some(linked_body)
                    }
                    None => None,
                };
                Expression::Match {
                    subject: Box::new(subject),
                    cases: linked_cases,
                    default,
                    span: *span,
                }
            }
            ParsedExpression::Closure(closure) => self.link_closure(
                &closure.parameters,
                &closure.return_type,
                &closure.body,
                closure.span,
                context,
            )?,
            ParsedExpression::MethodCall {
                receiver,
                method,
                method_span,
                arguments,
                span,
            } => {
                if let Some((ty, index)) = self.variant_member(context, receiver, method)? {
                    let mut payload = Vec::new();
                    for argument in arguments {
                        payload.push(self.link_expression(argument, context)?);
                    }
                    return Ok(Expression::Variant {
                        ty,
                        index,
                        payload,
                        span: *span,
                    });
                }
                // A type name as the receiver reaches an associated function:
                // the function in the type's `impl` block whose first parameter
                // is not the type. The type is not a value, so it is not an
                // argument.
                if let Some(declaration) = self.associated_member(context, receiver, method)? {
                    let mut linked_arguments = Vec::new();
                    for argument in arguments {
                        linked_arguments.push(self.link_expression(argument, context)?);
                    }
                    return Ok(Expression::Call {
                        callee: Box::new(Expression::Reference {
                            declaration,
                            span: *method_span,
                        }),
                        callee_span: *method_span,
                        arguments: linked_arguments,
                        span: *span,
                    });
                }
                let linked = self.link_expression(receiver, context)?;
                let receiver_head = self.receiver_type(&linked).map(|ty| ty.head(&self.types));
                // A `receiver.name(args)` call is a call to a function named
                // `name` whose first parameter accepts the receiver: the
                // receiver becomes the first argument. The name is resolved in
                // the caller's namespace and the namespaces it uses, exactly
                // like a bare name; among those, the receiver's type head
                // selects the one function whose first parameter has that head.
                let declaration =
                    self.select_method(context, method, receiver_head, *method_span)?;
                let Some(declaration) = declaration else {
                    return Err(Diagnostic::new(
                        &self.files[context.file.0].path,
                        *method_span,
                        format!("no method `{method}` for this value"),
                    ));
                };
                let mut linked_arguments = vec![linked];
                for argument in arguments {
                    linked_arguments.push(self.link_expression(argument, context)?);
                }
                Expression::Call {
                    callee: Box::new(Expression::Reference {
                        declaration,
                        span: *method_span,
                    }),
                    callee_span: *method_span,
                    arguments: linked_arguments,
                    span: *span,
                }
            }
            ParsedExpression::Name { path, span } => {
                let declaration = self.resolve(context, path, *span)?;
                Expression::Reference {
                    declaration,
                    span: *span,
                }
            }
            ParsedExpression::Call {
                callee,
                callee_span,
                arguments,
                span,
            } => {
                let mut linked_arguments = Vec::new();
                for argument in arguments {
                    linked_arguments.push(self.link_expression(argument, context)?);
                }
                // A call selects a method by its first argument's type head,
                // exactly as a method call selects by its receiver's; a call
                // whose first argument gives no head falls back to the name
                // pool.
                let head = linked_arguments
                    .first()
                    .and_then(|argument| self.receiver_type(argument))
                    .map(|ty| ty.head(&self.types));
                let declaration = self.resolve_call(context, callee, head, *callee_span)?;
                Expression::Call {
                    callee: Box::new(Expression::Reference {
                        declaration,
                        span: *callee_span,
                    }),
                    callee_span: *callee_span,
                    arguments: linked_arguments,
                    span: *span,
                }
            }
            ParsedExpression::Field {
                target,
                name,
                name_span,
                span,
            } => {
                // A bound function reached through its type as a value:
                // `Type.name` is the function value, with no call parentheses.
                if let Some(declaration) = self.bound_member(context, target, name)? {
                    return Ok(Expression::Reference {
                        declaration,
                        span: *name_span,
                    });
                }
                if let Some((ty, index)) = self.variant_member(context, target, name)? {
                    return Ok(Expression::Variant {
                        ty,
                        index,
                        payload: Vec::new(),
                        span: *span,
                    });
                }
                Expression::Field {
                    target: Box::new(self.link_expression(target, context)?),
                    name: name.clone(),
                    name_span: *name_span,
                    span: *span,
                }
            }
            ParsedExpression::Bool { value, span } => Expression::Bool {
                value: *value,
                span: *span,
            },
            ParsedExpression::Int { value, span } => Expression::Int {
                value: *value,
                span: *span,
            },
            ParsedExpression::Binary {
                op,
                left,
                right,
                span,
            } => Expression::Binary {
                op: *op,
                left: Box::new(self.link_expression(left, context)?),
                right: Box::new(self.link_expression(right, context)?),
                span: *span,
            },
            ParsedExpression::Unary { op, operand, span } => Expression::Unary {
                op: *op,
                operand: Box::new(self.link_expression(operand, context)?),
                span: *span,
            },
        })
    }

    /// Link an anonymous function literal into a function expression. Its body
    /// is linked with the enclosing scopes still open, so a reference to an
    /// outer local resolves and is recorded as a capture. Its parameters and
    /// locals belong to the nearest enclosing function, so a nested literal is
    /// the same kind of node as any other function.
    fn link_closure(
        &mut self,
        parameters: &[crate::syntax::Parameter],
        return_type: &Option<TypeExpr>,
        body: &[crate::syntax::Statement],
        span: Span,
        context: &mut Context,
    ) -> Result<Expression, Diagnostic> {
        context.scopes.push();
        let mut parameter_ids = Vec::new();
        let mut parameter_types = Vec::new();
        for parameter in parameters {
            let ty = self.resolve_type(&parameter.ty, context.namespace, context.file, &[])?;
            parameter_types.push(ty.clone());
            let id = self.declare_local(
                context.scopes,
                LocalDeclaration {
                    file: context.file,
                    namespace: context.namespace,
                    name: &parameter.name,
                    name_span: parameter.span,
                    owner: context.current,
                    ty,
                },
            )?;
            context.scopes.declare(&parameter.name, id);
            parameter_ids.push(id);
        }
        let linked_body = self.link_statements(body, context)?;
        context.scopes.pop();
        let returns = match return_type {
            Some(expr) => Some(Box::new(self.resolve_type(
                expr,
                context.namespace,
                context.file,
                &[],
            )?)),
            None => None,
        };
        let ty = Type::Function {
            parameters: parameter_types,
            returns,
        };
        // Capture only the enclosing locals the body reads, including those a
        // nested literal needs, not every local in scope.
        let captures = self.collect_captures(&linked_body, &parameter_ids);
        Ok(Expression::Function(Function {
            ty,
            parameters: parameter_ids,
            body: linked_body,
            type_parameters: Vec::new(),
            captures,
            span,
        }))
    }

    fn link_match_body(
        &mut self,
        body: &crate::syntax::MatchBody,
        context: &mut Context,
    ) -> Result<MatchBody, Diagnostic> {
        Ok(match body {
            crate::syntax::MatchBody::Expression(expression) => {
                MatchBody::Expression(Box::new(self.link_expression(expression, context)?))
            }
            crate::syntax::MatchBody::Block(statements) => {
                MatchBody::Block(self.link_statements(statements, context)?)
            }
        })
    }
}

/// What a local binding needs: where it was written, what it is called, which
/// function owns it, and the type it holds.
pub(super) struct LocalDeclaration<'a> {
    pub(super) file: FileId,
    pub(super) namespace: NamespaceId,
    pub(super) name: &'a str,
    pub(super) name_span: Span,
    pub(super) owner: DeclarationId,
    pub(super) ty: Type,
}
