//! Linking parsed bodies into model nodes.
//!
//! The linking pass walks each pending declaration's parsed body, creating
//! local bindings and resolving references and calls into edges to declaration
//! nodes. After linking, no phase resolves a name again.

use crate::compiler::Diagnostic;
use crate::compiler::compile::load::LoadedProgram;
use crate::compiler::model::Type;
use crate::compiler::model::{
    Declaration, DeclarationId, Expression, Facts, Field, FileId, Function, MatchArm, MatchBody,
    NamespaceId, Program, Statement, StringPart,
};
use crate::syntax::Span;
use crate::syntax::{Expression as ParsedExpression, Statement as ParsedStatement, TypeExpr};

use super::builder::{Builder, Pending};
use super::name_resolution::CalleeLink;
use super::scopes::{Context, Scopes};

/// The program one resolution builds: the model and the warnings its unused
/// syntax earned.
pub(crate) struct Analysis {
    pub(crate) program: Program,
    pub(crate) warnings: Vec<Diagnostic>,
}

/// Build the program model from the joined item stream. Every body is linked
/// and every module value ordered: a fact about one declaration must never
/// depend on which file the client is looking at.
pub(crate) fn resolve_tree(loaded: &LoadedProgram) -> Result<Analysis, Diagnostic> {
    let mut builder = prepare(loaded)?;
    builder.settle_types(loaded)?;
    builder.link_bodies(loaded)?;
    let module_values = builder.order_module_values(&builder.module_values)?;
    // The unused-`use` fact is settled: every body is linked and every read
    // recorded itself. It is read before `finish` takes the builder apart.
    let warnings = builder.use_warnings(loaded);
    let program = builder.finish(module_values);
    #[cfg(debug_assertions)]
    crate::compiler::model::assert_known_relations(
        &program,
        &Facts::default(),
        crate::compiler::model::RelationBoundary::Resolved,
    );
    Ok(Analysis { program, warnings })
}

/// Register the model's skeleton: the declared types, the natives, the
/// declarations, and the `use`s they bring. Both resolutions share this order.
fn prepare(loaded: &LoadedProgram) -> Result<Builder, Diagnostic> {
    let mut builder = Builder::new();
    // Reserve the declared types first, so a native row can name one by path,
    // then register the natives before the declarations that call them.
    builder.reserve_items(loaded)?;
    builder.register_builtins()?;
    // Index what the `use`s deliver before any declared type is filled: a
    // field or a receiver type may name a type the namespace only `use`s.
    builder.index_uses(loaded);
    builder.register_declarations(loaded)?;
    builder.check_uses(loaded)?;
    // The plain functions a `use` brings are registered by now, so the second
    // pass records the `use`s that reached nothing above.
    builder.index_uses(loaded);
    Ok(builder)
}

impl Builder {
    /// Settle every declaration's type. The first linking pass reads only the
    /// parsed signatures, so a body in the second pass can read any callee's
    /// signature whatever the declaration order: resolution is order-free.
    fn settle_types(&mut self, loaded: &LoadedProgram) -> Result<(), Diagnostic> {
        for pending in &self.pending {
            let (declaration_id, file, item, namespace) = pending.parts();
            let skeleton = &self.skeletons[declaration_id.0];
            let name = skeleton.name.clone();
            let scoped = &loaded.items[item];
            let declaration = &scoped.declaration;
            let ty = match pending {
                Pending::Callable { .. } => {
                    let function = declaration
                        .function()
                        .expect("a callable declaration carries a function body");
                    self.function_signature(function, namespace, file)?
                }
                Pending::ModuleValue { .. } => {
                    let binding = declaration
                        .binding()
                        .expect("a module value carries its initializer");
                    self.resolve_type(&binding.ty, namespace, file, &[])?
                }
            };
            self.declarations[declaration_id.0] = Declaration {
                // Registration normalized the loader's surface kind and
                // settled the function binding, so the model node holds those
                // facts rather than the parsed item's kind.
                kind: skeleton.kind,
                binding: skeleton.binding,
                name,
                name_span: skeleton.name_span,
                public: skeleton.public,
                file: Some(file),
                namespace,
                owner: None,
                ty,
                value: None,
                native: None,
            };
        }
        Ok(())
    }

    /// Link every body. Every declaration's type is already settled, so a
    /// body reads any callee's signature regardless of order.
    fn link_bodies(&mut self, loaded: &LoadedProgram) -> Result<(), Diagnostic> {
        for pending in std::mem::take(&mut self.pending) {
            let (declaration_id, file, item, namespace) = pending.parts();
            let declaration = &loaded.items[item].declaration;
            let value = match &pending {
                Pending::Callable { .. } => {
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
                        current: declaration_id,
                        type_parameters: &type_parameters,
                    };
                    let mut parameters = Vec::new();
                    for parameter in &function.parameters {
                        let ty =
                            self.resolve_type(&parameter.ty, namespace, file, &type_parameters)?;
                        let id = self.declare_local(
                            context.scopes,
                            LocalDeclaration {
                                file,
                                namespace,
                                name: &parameter.name,
                                name_span: parameter.span,
                                owner: declaration_id,
                                ty,
                            },
                        )?;
                        parameters.push(id);
                    }
                    let body = self.link_statements(&function.body, &mut context)?;
                    Expression::Function(Function {
                        declared_return: None,
                        parameters,
                        body,
                        type_parameters,
                        captures: Vec::new(),
                        span: function.span,
                    })
                }
                Pending::ModuleValue { .. } => {
                    let binding = declaration
                        .binding()
                        .expect("a module value carries its initializer");
                    let mut scopes = Scopes::default();
                    let mut context = Context {
                        scopes: &mut scopes,
                        namespace,
                        file,
                        current: declaration_id,
                        type_parameters: &[],
                    };
                    self.link_expression(&binding.value, &mut context)?
                }
            };
            self.declarations[declaration_id.0].value = Some(value);
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
        self.resolve_type_expr(expr, namespace, file, parameters, &self.files[file.0].path)
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
                if let Some(variant) = self.variant_member(context, receiver, method)? {
                    let ty = variant.ty;
                    let index = variant.index;
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
                // A `receiver.name(args)` call is a call to a function named
                // `name` whose first parameter accepts the receiver: the
                // receiver becomes the first argument. Which function that is
                // keys on the receiver's type head, and a head is a type fact
                // the check derives — so the linker writes the family alone
                // and the check resolves it.
                let mut linked_arguments = vec![linked];
                for argument in arguments {
                    linked_arguments.push(self.link_expression(argument, context)?);
                }
                Expression::Call {
                    callee: Box::new(Expression::Method {
                        namespace: None,
                        path: vec![method.clone()],
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
            // The wildcard alone names no declaration: it reaches expression
            // position out of a select few programs, and the link fails with
            // the unknown name its one word reports.
            ParsedExpression::Wildcard { span } => {
                return Err(self.unresolved(context, crate::syntax::WILDCARD, *span));
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
                // A call resolves through the name pools; a qualified name the
                // pools do not own may still be a bound method, whose receiver's
                // head selects it — a head is a type fact the check derives, so
                // the callee stays open as the family the path named.
                let callee = match self.resolve_call(context, callee, *callee_span)? {
                    CalleeLink::Declaration(declaration) => Expression::Reference {
                        declaration,
                        span: *callee_span,
                    },
                    CalleeLink::Family(namespace, path) => Expression::Method {
                        namespace: Some(namespace),
                        path,
                        span: *callee_span,
                    },
                };
                Expression::Call {
                    callee: Box::new(callee),
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
                if let Some(variant) = self.variant_member(context, target, name)? {
                    return Ok(Expression::Variant {
                        ty: variant.ty,
                        index: variant.index,
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
        // The literal is one anonymous function declaration. It is contained
        // by the body that writes it and owns no namespace name, so the
        // enclosing body holds a reference to it rather than the body itself.
        let declaration =
            self.declare_anonymous_function(context.current, context.file, context.namespace, span);
        // The body's parameters and locals belong to the anonymous function,
        // while the enclosing scopes stay open so an outer local still
        // resolves and is captured.
        let enclosing = context.current;
        context.current = declaration;
        context.scopes.push();
        let mut parameter_ids = Vec::new();
        for parameter in parameters {
            let ty = self.resolve_type(&parameter.ty, context.namespace, context.file, &[])?;
            let id = self.declare_local(
                context.scopes,
                LocalDeclaration {
                    file: context.file,
                    namespace: context.namespace,
                    name: &parameter.name,
                    name_span: parameter.span,
                    owner: declaration,
                    ty,
                },
            )?;
            parameter_ids.push(id);
        }
        let linked_body = self.link_statements(body, context)?;
        context.scopes.pop();
        context.current = enclosing;
        let declared_return = match return_type {
            Some(expr) => Some(self.resolve_type(expr, context.namespace, context.file, &[])?),
            None => None,
        };
        // Capture only the enclosing locals the body reads, including those a
        // nested literal needs, not every local in scope.
        let captures = self.collect_captures(&linked_body, &parameter_ids);
        let ty = Type::Function {
            parameters: parameter_ids
                .iter()
                .map(|parameter| self.declarations[parameter.0].ty.clone())
                .collect(),
            returns: declared_return.clone().map(Box::new),
        };
        self.declarations[declaration.0].ty = ty;
        self.declarations[declaration.0].value = Some(Expression::Function(Function {
            declared_return,
            parameters: parameter_ids,
            body: linked_body,
            type_parameters: Vec::new(),
            captures,
            span,
        }));
        Ok(Expression::Reference { declaration, span })
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
