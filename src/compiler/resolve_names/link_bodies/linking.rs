//! Linking parsed bodies into model nodes.
//!
//! The linking pass walks each pending declaration's parsed body, creating
//! local bindings and resolving references and calls into edges to declaration
//! nodes. After linking, no phase resolves a name again.

use crate::compiler::load_files::LoadedProgram;
use crate::compiler::{Diagnostic, duplicate_name};
use crate::model::{
    BindingKind, Case, Declaration, DeclarationId, DeclarationKind, Derived, Expression, Field,
    FileId, Function, Kind, NamespaceId, Origin, Program, Registry, Statement,
};
use crate::syntax::Span;
use crate::syntax::{
    Declaration as ParsedDeclaration, Expression as ParsedExpression, Statement as ParsedStatement,
    ValueKind,
};

use super::super::register_declarations::{Builder, Skeleton};
use super::scopes::{Context, Scopes};

/// Build the program model. On failure the parsed program is left untouched
/// except for the declarations already consumed, so the caller can still
/// render the diagnostic against its sources.
pub(crate) fn resolve_names(loaded: &mut LoadedProgram) -> Result<Program, Diagnostic> {
    let entry_file = FileId(loaded.entry);
    let parsed: Vec<Vec<ParsedDeclaration>> = loaded
        .files
        .iter_mut()
        .map(|file| std::mem::take(&mut file.file.declarations))
        .collect();

    let mut builder = Builder::new();
    builder.register_builtins();
    for (index, file) in loaded.files.iter().enumerate() {
        if file.origin == Origin::Embedded {
            builder.register_file(loaded, index, &parsed[index])?;
        }
    }
    for index in loaded.ordered_files() {
        builder.register_file(loaded, index, &parsed[index])?;
    }

    let entry = builder.resolve_entry(loaded, entry_file, &parsed)?;
    builder.link_declarations(&parsed)?;
    let program = builder.finish(entry);
    crate::model::verify_program(&program);
    Ok(program)
}

impl Builder {
    fn link_declarations(&mut self, parsed: &[Vec<ParsedDeclaration>]) -> Result<(), Diagnostic> {
        for pending in std::mem::take(&mut self.pending) {
            let skeleton = &self.skeletons[pending.declaration.0];
            let name = skeleton.name.clone();
            let name_span = skeleton.name_span;
            let file = pending.file;
            let namespace = pending.namespace;
            let order = pending.order;
            let declaration = &parsed[file.0][pending.syntax_index];

            let (kind, parameters, seeded_kind) = match declaration {
                ParsedDeclaration::Function(function) => {
                    let mut scopes = Scopes::default();
                    scopes.push();
                    let mut context = Context {
                        scopes: &mut scopes,
                        namespace,
                        file,
                        current: pending.declaration,
                        current_is_function: true,
                        order,
                    };
                    let mut parameters = Vec::new();
                    for parameter in &function.parameters {
                        let id = self.declare_local(
                            context.scopes,
                            LocalDeclaration {
                                file,
                                namespace,
                                name: &parameter.name,
                                name_span: parameter.span,
                                owner: pending.declaration,
                                binding: BindingKind::Parameter {
                                    mutable: parameter.mutable,
                                },
                                declared_kind: Some(declared_kind(parameter.kind)),
                            },
                        )?;
                        context.scopes.declare(&parameter.name, id);
                        parameters.push(id);
                    }
                    let body = self.link_statements(&function.body, &mut context)?;
                    let return_kind = function
                        .return_kind
                        .map(declared_kind)
                        .unwrap_or(Kind::Nothing);
                    (
                        DeclarationKind::Function(Function { body, return_kind }),
                        parameters,
                        Some(return_kind),
                    )
                }
                ParsedDeclaration::Binding(binding) => {
                    let mut scopes = Scopes::default();
                    let mut context = Context {
                        scopes: &mut scopes,
                        namespace,
                        file,
                        current: pending.declaration,
                        current_is_function: false,
                        order,
                    };
                    let expression = self.link_expression(&binding.value, &mut context)?;
                    let kind = match binding.kind {
                        ValueKind::Text => DeclarationKind::Text(expression),
                        ValueKind::Record => DeclarationKind::Record(expression),
                        ValueKind::List => DeclarationKind::List(expression),
                    };
                    (kind, Vec::new(), None)
                }
            };
            self.declarations[pending.declaration.0] = Some(Declaration {
                name,
                name_span,
                file: Some(file),
                namespace,
                order,
                owner: None,
                parameters,
                kind,
                derived: Derived {
                    kind: seeded_kind,
                    halts: false,
                },
            });
        }
        Ok(())
    }

    /// Create a local binding node owned by a function, rejecting a name that
    /// is already visible in this body. A local may shadow a module name: a
    /// bare name reads the local first, and `::` reaches the module name.
    fn declare_local(
        &mut self,
        scopes: &Scopes,
        local: LocalDeclaration<'_>,
    ) -> Result<DeclarationId, Diagnostic> {
        let LocalDeclaration {
            file,
            namespace,
            name,
            name_span,
            owner,
            binding,
            declared_kind,
        } = local;
        let reported = &self.files[file.0].path;
        if scopes.find(name).is_some() {
            return Err(Diagnostic::new(reported, name_span, duplicate_name(name)));
        }
        let order = self.skeletons[owner.0].order;
        let id = self.create_node(
            Skeleton {
                name: name.to_owned(),
                name_span,
                file: None,
                namespace,
                order,
                syntax_index: None,
            },
            Some(owner),
            Some(DeclarationKind::Binding(binding)),
            declared_kind,
        );
        Ok(id)
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
                    let binding_kind = match binding.kind {
                        ValueKind::Text => BindingKind::Text {
                            mutable: binding.mutable,
                        },
                        ValueKind::Record => BindingKind::Record {
                            mutable: binding.mutable,
                        },
                        ValueKind::List => BindingKind::List {
                            mutable: binding.mutable,
                        },
                    };
                    let declaration = self.declare_local(
                        context.scopes,
                        LocalDeclaration {
                            file: context.file,
                            namespace: context.namespace,
                            name: &binding.name,
                            name_span: binding.name_span,
                            owner: context.current,
                            binding: binding_kind,
                            declared_kind: None,
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
                    let declaration = self.resolve(
                        context,
                        std::slice::from_ref(name),
                        false,
                        *name_span,
                        Registry::Value,
                    )?;
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
                    let declaration = self.resolve(
                        context,
                        std::slice::from_ref(name),
                        false,
                        *name_span,
                        Registry::Value,
                    )?;
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
                ParsedStatement::Switch {
                    subject,
                    cases,
                    default,
                    span,
                } => {
                    let subject = self.link_expression(subject, context)?;
                    let mut linked_cases = Vec::new();
                    for case in cases {
                        context.scopes.push();
                        let pattern = self.link_expression(&case.pattern, context)?;
                        let body = self.link_statements(&case.body, context)?;
                        context.scopes.pop();
                        linked_cases.push(Case {
                            pattern,
                            body,
                            span: case.span,
                        });
                    }
                    let default = match default {
                        Some(body) => {
                            context.scopes.push();
                            let linked_body = self.link_statements(body, context)?;
                            context.scopes.pop();
                            Some(linked_body)
                        }
                        None => None,
                    };
                    linked.push(Statement::Switch {
                        subject,
                        cases: linked_cases,
                        default,
                        span: *span,
                    });
                }
                ParsedStatement::ForEach {
                    item,
                    item_span,
                    iterable,
                    body,
                    span,
                } => {
                    let iterable = self.link_expression(iterable, context)?;
                    context.scopes.push();
                    let declaration = self.declare_local(
                        context.scopes,
                        LocalDeclaration {
                            file: context.file,
                            namespace: context.namespace,
                            name: item,
                            name_span: *item_span,
                            owner: context.current,
                            binding: BindingKind::Text { mutable: false },
                            declared_kind: Some(Kind::Text),
                        },
                    )?;
                    context.scopes.declare(item, declaration);
                    let body = self.link_statements(body, context)?;
                    context.scopes.pop();
                    linked.push(Statement::ForEach {
                        item: declaration,
                        iterable,
                        body,
                        span: *span,
                    });
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

    fn link_expression(
        &mut self,
        expression: &ParsedExpression,
        context: &mut Context,
    ) -> Result<Expression, Diagnostic> {
        Ok(match expression {
            ParsedExpression::Text { value, span } => Expression::Text {
                value: value.clone(),
                span: *span,
            },
            ParsedExpression::Record { fields, span } => Expression::Record {
                fields: self.link_fields(fields, context)?,
                span: *span,
            },
            ParsedExpression::List { elements, span } => {
                let mut linked_elements = Vec::new();
                for element in elements {
                    linked_elements.push(self.link_expression(element, context)?);
                }
                Expression::List {
                    elements: linked_elements,
                    span: *span,
                }
            }
            ParsedExpression::Name { root, path, span } => Expression::Reference {
                declaration: self.resolve(context, path, *root, *span, Registry::Value)?,
                span: *span,
            },
            ParsedExpression::Call {
                root,
                callee,
                callee_span,
                arguments,
                span,
            } => {
                let declaration =
                    self.resolve(context, callee, *root, *callee_span, Registry::Function)?;
                let mut linked_arguments = Vec::new();
                for argument in arguments {
                    linked_arguments.push(self.link_expression(argument, context)?);
                }
                Expression::Call {
                    callee: declaration,
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
            } => Expression::Field {
                target: Box::new(self.link_expression(target, context)?),
                name: name.clone(),
                name_span: *name_span,
                span: *span,
            },
            ParsedExpression::Add { left, right, span } => Expression::Add {
                left: Box::new(self.link_expression(left, context)?),
                right: Box::new(self.link_expression(right, context)?),
                span: *span,
            },
        })
    }
}

/// What a local binding needs: where it was written, what it is called, which
/// function owns it, and the kind it declares when the source writes one.
struct LocalDeclaration<'a> {
    file: FileId,
    namespace: NamespaceId,
    name: &'a str,
    name_span: Span,
    owner: DeclarationId,
    binding: BindingKind,
    /// The kind written in the source, only for a parameter.
    declared_kind: Option<Kind>,
}

/// The kind a written keyword names, on a parameter or a return type. This is
/// the only mapping from a written kind to the kind system.
fn declared_kind(kind: ValueKind) -> Kind {
    match kind {
        ValueKind::Text => Kind::Text,
        ValueKind::Record => Kind::Record,
        ValueKind::List => Kind::List,
    }
}
