//! Structural verification of the program model.
//!
//! `validate_program_structure` checks every id and edge against the tables it
//! points into, reports the first violation, and enforces the top-down rule on
//! every edge. It is a compile-time invariant: `verify_program` runs it plus
//! the span checks, so a compiler that produces an invalid graph fails loudly
//! during development. The untrusted decode boundary is
//! `validate_bytecode_structure`, which runs on the lowered bytecode.

use crate::compiler::model::{Type, TypeHead};
use crate::compiler::native_registry::native_shape;

use super::Program;
use super::declarations::{DeclarationKind, FunctionBinding, TypeKind};
use super::expressions::{Expression, MatchBody, Pattern, Statement};
use super::ids::DeclarationId;
use super::verify_spans::verify_spans;
use super::visitors::{Visitor, walk_expression, walk_statements};

/// Assert the program is structurally valid. A violation is a compiler bug.
pub(crate) fn verify_program(program: &Program) {
    if let Err(violation) = validate_program_structure(program) {
        panic!("program structure is invalid: {violation}");
    }
    verify_spans(program);
}

/// Check every id and edge in the program, returning the first violation.
pub(crate) fn validate_program_structure(program: &Program) -> Result<(), String> {
    let declaration_count = program.declarations.len();
    let file_count = program.files.len();
    let namespace_count = program.namespaces.len();

    // The entry's own id is guaranteed by selection, so only the table-wide
    // edges are checked here.
    for id in &program.module_values {
        check_id(id.0, declaration_count, "a module value")?;
    }
    if program.namespace(program.root()).parent.is_some() {
        return Err("the root namespace has a parent".to_owned());
    }
    if program.namespace_at(&[]).is_none() {
        return Err("the root namespace is missing".to_owned());
    }
    if let Some(list) = program.list_type() {
        check_id(list.0, declaration_count, "the runtime list type")?;
        let body = program.declarations[list.0]
            .type_body()
            .ok_or("the runtime list type is not a type declaration")?;
        if body.element_parameter().is_none() {
            return Err("the runtime list does not declare one element parameter".to_owned());
        }
    }

    for namespace in &program.namespaces {
        if let Some(parent) = namespace.parent {
            check_id(parent.0, namespace_count, "a namespace parent")?;
        }
        for child in namespace.children.values() {
            check_id(child.0, namespace_count, "a namespace child")?;
        }
        for declaration in namespace.names.iter() {
            check_id(declaration.0, declaration_count, "a registered declaration")?;
        }
        for declaration in namespace.types.iter() {
            check_id(
                declaration.0,
                declaration_count,
                "a registered type declaration",
            )?;
        }
        for declaration in namespace.methods.iter() {
            check_id(declaration.0, declaration_count, "a registered method")?;
        }
        for declaration in namespace.associates.iter() {
            check_id(
                declaration.0,
                declaration_count,
                "a registered associated function",
            )?;
        }
    }
    for file in &program.files {
        for declaration in &file.declarations {
            check_id(declaration.0, declaration_count, "a file declaration")?;
        }
    }
    for declaration in &program.declarations {
        if let Some(file) = declaration.file {
            check_id(file.0, file_count, "a declaration file")?;
        }
        check_id(
            declaration.namespace.0,
            namespace_count,
            "a declaration namespace",
        )?;
        if let Some(owner) = declaration.owner {
            check_id(owner.0, declaration_count, "a declaration owner")?;
        }
        match declaration.binding {
            // Only a function binds: a named one to a namespace name, an
            // anonymous one to the body that writes its literal, and a method
            // or associated function to the type head that files it.
            Some(FunctionBinding::Named) if declaration.kind != DeclarationKind::Function => {
                return Err("a non-function declaration names a named binding".to_owned());
            }
            Some(FunctionBinding::Anonymous { container }) => {
                if declaration.kind != DeclarationKind::Function {
                    return Err("a non-function declaration names an anonymous binding".to_owned());
                }
                check_id(
                    container.0,
                    declaration_count,
                    "an anonymous function's container",
                )?;
            }
            Some(FunctionBinding::Method { receiver }) => {
                if declaration.kind != DeclarationKind::Function {
                    return Err(
                        "a non-function declaration names a bound-function binding".to_owned()
                    );
                }
                check_head(program, receiver, "a method receiver")?;
            }
            Some(FunctionBinding::Associated { owner }) => {
                if declaration.kind != DeclarationKind::Function {
                    return Err(
                        "a non-function declaration names a bound-function binding".to_owned()
                    );
                }
                check_head(program, owner, "an associated function's type")?;
            }
            Some(FunctionBinding::Named) | None => {}
        }
        check_type(program, &declaration.ty, "a declaration type")?;
        match (&declaration.kind, &declaration.type_body) {
            (DeclarationKind::Rec | DeclarationKind::Enum, None) => {
                return Err("a type declaration carries no type body".to_owned());
            }
            (DeclarationKind::Rec | DeclarationKind::Enum, Some(body)) => match &body.kind {
                TypeKind::Rec(fields) => {
                    if declaration.kind != DeclarationKind::Rec {
                        return Err("a rec body belongs to an enum declaration".to_owned());
                    }
                    for field in fields {
                        check_type(program, &field.ty, "a type field")?;
                    }
                }
                TypeKind::Enum(variants) => {
                    if declaration.kind != DeclarationKind::Enum {
                        return Err("an enum body belongs to a rec declaration".to_owned());
                    }
                    for variant in variants {
                        for ty in &variant.payload {
                            check_type(program, ty, "a variant payload")?;
                        }
                    }
                }
            },
            (_, Some(_)) => {
                return Err("a value declaration carries a type body".to_owned());
            }
            (_, None) => {}
        }
        if declaration.kind.callable() && declaration.kind.has_body() {
            let function = declaration
                .function()
                .expect("a callable body is a function expression");
            for parameter in &function.parameters {
                check_id(parameter.0, declaration_count, "a function parameter")?;
            }
            for capture in &function.captures {
                check_id(capture.0, declaration_count, "a function capture")?;
            }
            check_statements(program, &function.body)?;
        } else if declaration.kind.module_value() {
            check_expression(
                program,
                declaration
                    .value()
                    .expect("a module value carries its initializer"),
            )?;
        } else if let Some(native) = declaration.native {
            native_shape(native);
        }
    }
    Ok(())
}

/// Check a type head names a type declaration.
fn check_head(program: &Program, head: TypeHead, what: &str) -> Result<(), String> {
    if let TypeHead::Named(id) = head {
        check_named_type(program, id, what)?;
    }
    Ok(())
}

/// Reject a `Named` type whose id points past the declaration table or names a
/// declaration that is not a type, and check its arguments recursively. A
/// named type applies as many arguments as its body declares parameters.
fn check_type(program: &Program, ty: &Type, what: &str) -> Result<(), String> {
    match ty {
        Type::Named(id, arguments) => {
            let body = check_named_type(program, *id, what)?;
            if arguments.len() != body.parameters.len() {
                return Err(format!(
                    "{what} applies {} argument(s) to a type that declares {}",
                    arguments.len(),
                    body.parameters.len()
                ));
            }
            for argument in arguments {
                check_type(program, argument, what)?;
            }
        }
        Type::Function {
            parameters,
            returns,
        } => {
            for parameter in parameters {
                check_type(program, parameter, what)?;
            }
            if let Some(returns) = returns {
                check_type(program, returns, what)?;
            }
        }
        // The leaf variants carry no nested type, so there is nothing below
        // them to check. They are listed so a new variant is a compile error.
        Type::Text | Type::Bool | Type::Int | Type::Parameter(_) => {}
    }
    Ok(())
}

/// The type body a named type id names, or an error when the id points at no
/// type declaration.
fn check_named_type<'a>(
    program: &'a Program,
    id: DeclarationId,
    what: &str,
) -> Result<&'a super::declarations::TypeBody, String> {
    let declaration = program
        .declarations
        .get(id.0)
        .ok_or_else(|| format!("{what} points past the declarations"))?;
    if !matches!(
        declaration.kind,
        DeclarationKind::Rec | DeclarationKind::Enum
    ) {
        return Err(format!(
            "{what} names declaration id {}, which is not a type declaration",
            id.0
        ));
    }
    declaration.type_body().ok_or_else(|| {
        format!(
            "{what} names declaration id {}, which has no type body",
            id.0
        )
    })
}

/// Reject an id that points past the table it indexes.
fn check_id(id: usize, count: usize, what: &str) -> Result<(), String> {
    if id < count {
        Ok(())
    } else {
        Err(format!("{what} points past its table"))
    }
}

/// Walk a body and check every reference and call edge against the model.
fn check_statements(program: &Program, statements: &[Statement]) -> Result<(), String> {
    let mut check = EdgeCheck {
        program,
        error: None,
    };
    walk_statements(&mut check, statements);
    check.error.map_or(Ok(()), Err)
}

fn check_expression(program: &Program, expression: &Expression) -> Result<(), String> {
    let mut check = EdgeCheck {
        program,
        error: None,
    };
    walk_expression(&mut check, expression);
    check.error.map_or(Ok(()), Err)
}

/// Checks every reference and call edge points at a declaration. Resolution is
/// order-free, so no edge is checked for position: any declaration in the
/// program may be named. Every node kind is listed, so a new edge-bearing kind
/// fails to compile.
struct EdgeCheck<'a> {
    program: &'a Program,
    error: Option<String>,
}

impl EdgeCheck<'_> {
    fn check(&mut self, declaration: DeclarationId, what: &str) {
        if self.error.is_none() && declaration.0 >= self.program.declarations.len() {
            self.error = Some(format!("{what} points past the declarations"));
        }
    }

    /// Check a variant's declared type points at a type declaration.
    fn check_type(&mut self, ty: DeclarationId) {
        if self.error.is_none()
            && self
                .program
                .declarations
                .get(ty.0)
                .and_then(|declaration| declaration.type_body())
                .is_none()
        {
            self.error = Some("a variant type points past the type declarations".to_owned());
        }
    }
}

impl EdgeCheck<'_> {
    /// Check the declaration a binding statement declares.
    fn bound(&mut self, statement: &Statement) {
        let (Statement::Bind { declaration, .. }
        | Statement::Assign { declaration, .. }
        | Statement::FieldAssign { declaration, .. }) = statement
        else {
            unreachable!("the walk names a binding statement");
        };
        self.check(*declaration, "a statement binding");
    }
}

impl Visitor for EdgeCheck<'_> {
    fn reference(&mut self, expression: &Expression) {
        let Expression::Reference { declaration, .. } = expression else {
            unreachable!("the walk names a reference");
        };
        self.check(*declaration, "a reference");
    }

    fn call(&mut self, expression: &Expression) {
        let Expression::Call { callee, .. } = expression else {
            unreachable!("the walk names a call");
        };
        // A direct call names a callable declaration; a call through a value
        // is a reference like any other. A family callee names no declaration
        // until the check resolves it.
        if let Expression::Reference { declaration, .. } = callee.as_ref()
            && self.program.declaration(*declaration).kind.callable()
        {
            self.check(*declaration, "a call");
        }
    }

    fn variant(&mut self, expression: &Expression) {
        let Expression::Variant { ty, .. } = expression else {
            unreachable!("the walk names a variant");
        };
        self.check_type(*ty);
    }

    // A function literal owns its parameters and reads its captures.
    fn function(&mut self, expression: &Expression) {
        let Expression::Function(function) = expression else {
            unreachable!("the walk names a function");
        };
        for parameter in &function.parameters {
            self.check(*parameter, "a function parameter");
        }
        for capture in &function.captures {
            self.check(*capture, "a reference");
        }
    }

    fn bind(&mut self, statement: &Statement) {
        self.bound(statement);
    }
    fn assign(&mut self, statement: &Statement) {
        self.bound(statement);
    }
    fn field_assign(&mut self, statement: &Statement) {
        self.bound(statement);
    }

    fn variant_pattern(&mut self, pattern: &Pattern) {
        let Pattern::Variant { ty, .. } = pattern else {
            unreachable!("the walk names a variant pattern");
        };
        self.check_type(*ty);
    }

    fn text(&mut self, _: &Expression) {}
    fn map(&mut self, _: &Expression) {}
    fn boolean(&mut self, _: &Expression) {}
    fn integer(&mut self, _: &Expression) {}
    fn interpolated(&mut self, _: &Expression) {}
    fn field(&mut self, _: &Expression) {}
    fn method(&mut self, _: &Expression) {}
    fn binary(&mut self, _: &Expression) {}
    fn unary(&mut self, _: &Expression) {}
    fn match_expression(&mut self, _: &Expression) {}
    fn expression_statement(&mut self, _: &Statement) {}
    fn return_statement(&mut self, _: &Statement) {}
    fn panic(&mut self, _: &Statement) {}
    fn asynchronous(&mut self, _: &Statement) {}
    fn wait(&mut self, _: &Statement) {}
    fn forever(&mut self, _: &Statement) {}
    fn break_statement(&mut self, _: &Statement) {}
    fn value_pattern(&mut self, _: &Pattern) {}
    fn expression_body(&mut self, _: &MatchBody) {}
    fn block_body(&mut self, _: &MatchBody) {}

    fn enters_deferred_body(&self) -> bool {
        true
    }
}
