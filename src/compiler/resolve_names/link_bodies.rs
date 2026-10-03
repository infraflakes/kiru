//! Linking bodies: resolve every name into an edge and build the model.
//!
//! The linking pass walks each pending declaration's parsed body, creating
//! local bindings and resolving references and calls into edges to
//! declaration nodes. After linking, no phase resolves a name again.

use std::collections::HashMap;

use crate::compiler::load_files::{LoadedProgram, Origin};
use crate::compiler::{
    BUILTIN_NAMESPACE, BindingKind, Case, Declaration, DeclarationId, DeclarationKind, Derived,
    Diagnostic, Expression, Field, FileId, Function, Kind, NamespaceId, Program, Statement,
    namespace_path,
};
use crate::syntax::Span;
use crate::syntax::{
    Declaration as ParsedDeclaration, Expression as ParsedExpression, ParameterKind,
    Statement as ParsedStatement,
};

use super::register_declarations::{Builder, Registry, Skeleton};

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
    super::check_invariants::verify(&program);
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

            let (kind, parameters) = match declaration {
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
                                binding: BindingKind::Parameter,
                                declared_kind: Some(parameter_kind(parameter.kind)),
                            },
                        )?;
                        context.scopes.declare(&parameter.name, id);
                        parameters.push(id);
                    }
                    let body = self.link_statements(&function.body, &mut context)?;
                    (DeclarationKind::Function(Function { body }), parameters)
                }
                ParsedDeclaration::Text(text) => {
                    let mut scopes = Scopes::default();
                    let mut context = Context {
                        scopes: &mut scopes,
                        namespace,
                        file,
                        current: pending.declaration,
                        current_is_function: false,
                        order,
                    };
                    (
                        DeclarationKind::Text(self.link_expression(&text.value, &mut context)?),
                        Vec::new(),
                    )
                }
                ParsedDeclaration::Rec(record) => {
                    let mut scopes = Scopes::default();
                    let mut context = Context {
                        scopes: &mut scopes,
                        namespace,
                        file,
                        current: pending.declaration,
                        current_is_function: false,
                        order,
                    };
                    (
                        DeclarationKind::Record(self.link_expression(&record.value, &mut context)?),
                        Vec::new(),
                    )
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
                derived: Derived::default(),
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
            return Err(Diagnostic::new(
                reported,
                name_span,
                format!("`{name}` is declared more than once"),
            ));
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
                ParsedStatement::Text(text) => {
                    let value = self.link_expression(&text.value, context)?;
                    let binding = self.declare_local(
                        context.scopes,
                        LocalDeclaration {
                            file: context.file,
                            namespace: context.namespace,
                            name: &text.name,
                            name_span: text.name_span,
                            owner: context.current,
                            binding: BindingKind::Text,
                            declared_kind: None,
                        },
                    )?;
                    context.scopes.declare(&text.name, binding);
                    linked.push(Statement::Bind {
                        declaration: binding,
                        value,
                        span: text.span,
                    });
                }
                ParsedStatement::Rec(record) => {
                    let value = self.link_expression(&record.value, context)?;
                    let binding = self.declare_local(
                        context.scopes,
                        LocalDeclaration {
                            file: context.file,
                            namespace: context.namespace,
                            name: &record.name,
                            name_span: record.name_span,
                            owner: context.current,
                            binding: BindingKind::Record,
                            declared_kind: None,
                        },
                    )?;
                    context.scopes.declare(&record.name, binding);
                    linked.push(Statement::Bind {
                        declaration: binding,
                        value,
                        span: record.span,
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
                ParsedStatement::Defer { body, span } => {
                    // A defer body is its own declaration scope: a name it
                    // declares does not leak into the enclosing body, though it
                    // may still assign the enclosing body's bindings.
                    context.scopes.push();
                    let linked_body = self.link_statements(body, context)?;
                    context.scopes.pop();
                    linked.push(Statement::Defer {
                        body: linked_body,
                        span: *span,
                    });
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
            ParsedExpression::Name { root, path, span } => Expression::Reference {
                declaration: self.resolve(context, path, *root, *span, Registry::Value)?,
                root: *root,
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
                    root: *root,
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

    /// Resolve a name to a node: a local binding (a value only), then the
    /// namespace registry `registry` names. A rooted path starts at the root
    /// namespace; a bare single segment climbs outward. A function cannot
    /// reference itself and a forward reference is an error, so a function
    /// can only call functions declared before it.
    fn resolve(
        &self,
        context: &Context,
        path: &[String],
        root: bool,
        span: Span,
        registry: Registry,
    ) -> Result<DeclarationId, Diagnostic> {
        let reported = &self.files[context.file.0].path;
        // A bare value reads a local first; a local never names a function.
        if !root
            && path.len() == 1
            && matches!(registry, Registry::Value)
            && let Some(found) = context.scopes.find(&path[0])
        {
            return Ok(found);
        }

        let display = display_path(path, root);
        let Some(found) = self.search(context, path, root, span, registry)? else {
            return Err(self.unknown_name(context, path, root, span, registry));
        };
        if found == context.current {
            let message = if context.current_is_function {
                "a function cannot reference itself"
            } else {
                "a value cannot reference itself"
            };
            return Err(Diagnostic::new(reported, span, message));
        }
        let skeleton = &self.skeletons[found.0];
        if skeleton.order > context.order {
            return Err(Diagnostic::new(
                reported,
                span,
                format!("`{display}` is declared after this point"),
            ));
        }
        Ok(found)
    }

    /// Search one registry for a path: climb outward from the current
    /// namespace for a bare single segment, otherwise start at the root
    /// namespace the path names.
    fn search(
        &self,
        context: &Context,
        path: &[String],
        root: bool,
        span: Span,
        registry: Registry,
    ) -> Result<Option<DeclarationId>, Diagnostic> {
        let (name, search) = path
            .split_last()
            .expect("a name path has at least one segment");
        if !root && search.is_empty() {
            let mut namespace = Some(context.namespace);
            while let Some(current) = namespace {
                if let Some(found) = self.registry_lookup(current, name, registry) {
                    return Ok(Some(found));
                }
                namespace = self.namespaces[current.0].parent;
            }
            return Ok(None);
        }
        let namespace = if search.is_empty() {
            NamespaceId(0)
        } else {
            match self.namespace_at(search) {
                Some(namespace) => {
                    if !self.namespace_reachable(context.file, namespace) {
                        return Err(Diagnostic::new(
                            &self.files[context.file.0].path,
                            span,
                            format!("namespace `{}` is not imported", search.join("::")),
                        ));
                    }
                    namespace
                }
                None => return Ok(None),
            }
        };
        Ok(self.registry_lookup(namespace, name, registry))
    }

    /// The diagnostic for a name no registry holds, with a hint when the
    /// other registry of the same search scope does hold it.
    fn unknown_name(
        &self,
        context: &Context,
        path: &[String],
        root: bool,
        span: Span,
        registry: Registry,
    ) -> Diagnostic {
        let other = match registry {
            Registry::Value => Registry::Function,
            Registry::Function => Registry::Value,
        };
        let display = display_path(path, root);
        let message = if self
            .search(context, path, root, span, other)
            .ok()
            .flatten()
            .is_some()
        {
            match registry {
                Registry::Value => format!("`{display}` is a function; call it"),
                Registry::Function => format!("`{display}` is a value and cannot be called"),
            }
        } else {
            format!("unknown name `{display}`")
        };
        Diagnostic::new(&self.files[context.file.0].path, span, message)
    }

    /// Look one name up in one registry of one namespace.
    fn registry_lookup(
        &self,
        namespace: NamespaceId,
        name: &str,
        registry: Registry,
    ) -> Option<DeclarationId> {
        match registry {
            Registry::Value => self.namespaces[namespace.0].value(name),
            Registry::Function => self.namespaces[namespace.0].function(name),
        }
    }

    fn namespace_at(&self, path: &[String]) -> Option<NamespaceId> {
        let mut current = NamespaceId(0);
        for segment in path {
            current = *self.namespaces[current.0].children.get(segment)?;
        }
        Some(current)
    }

    /// Whether a file may reach a namespace: its own, the implicit `std`
    /// tree, or a namespace of a file it imports directly.
    fn namespace_reachable(&self, file: FileId, namespace: NamespaceId) -> bool {
        if namespace == NamespaceId(0) {
            return true;
        }
        if namespace_path(&self.namespaces, namespace)
            .first()
            .map(String::as_str)
            == BUILTIN_NAMESPACE.first().copied()
        {
            return true;
        }
        let file = &self.files[file.0];
        if file.namespace == namespace {
            return true;
        }
        file.imports
            .iter()
            .any(|import| self.files[import.0].namespace == namespace)
    }
}

/// A name path as the diagnostics show it: a rooted path keeps its leading
/// `::`.
fn display_path(path: &[String], root: bool) -> String {
    let joined = path.join("::");
    if root { format!("::{joined}") } else { joined }
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

/// The kind a written parameter keyword names. This is the only mapping from a
/// parameter's declaration to the kind system.
fn parameter_kind(kind: ParameterKind) -> Kind {
    match kind {
        ParameterKind::Text => Kind::Text,
        ParameterKind::Record => Kind::Record,
    }
}

/// The context a body is linked in: the open scopes, the namespace and
/// declaration being linked, and the top-down rule they enforce.
struct Context<'a> {
    scopes: &'a mut Scopes,
    namespace: NamespaceId,
    file: FileId,
    current: DeclarationId,
    /// Whether the declaration being linked is a function, for the
    /// self-reference diagnostic.
    current_is_function: bool,
    order: usize,
}

/// The open scopes of one function body.
#[derive(Default)]
struct Scopes {
    stack: Vec<HashMap<String, DeclarationId>>,
}

impl Scopes {
    fn push(&mut self) {
        self.stack.push(HashMap::new());
    }

    fn pop(&mut self) {
        self.stack.pop();
    }

    fn declare(&mut self, name: &str, declaration: DeclarationId) {
        self.stack
            .last_mut()
            .expect("a scope is open while linking a body")
            .insert(name.to_owned(), declaration);
    }

    fn find(&self, name: &str) -> Option<DeclarationId> {
        self.stack
            .iter()
            .rev()
            .find_map(|scope| scope.get(name).copied())
    }
}
