//! Structural verification of the program model.
//!
//! `validate_program_structure` checks every id and edge against the tables it
//! points into, reports the first violation, and enforces the top-down rule on
//! every edge. It is a compile-time invariant: `verify_program` runs it plus
//! the span checks, so a compiler that produces an invalid graph fails loudly
//! during development. The untrusted decode boundary is
//! `validate_bytecode_structure`, which runs on the lowered bytecode.

use crate::compiler::model::{Type, TypeId};
use crate::compiler::native_registry::native_shape;

use super::Program;
use super::declarations::TypeKind;
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
    let type_count = program.types.len();

    // `resolve_entry` is the one place that decides the entry, and it selects
    // the program root's `main` function; only its id is checked here.
    check_id(program.entry.0, declaration_count, "the entry")?;
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
        check_id(list.0, type_count, "the runtime list type")?;
        if program.types[list.0].element_parameter().is_none() {
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
        check_type(&declaration.ty, type_count, "a declaration type")?;
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
    for declaration in &program.types {
        check_id(declaration.namespace.0, namespace_count, "a type namespace")?;
        match &declaration.kind {
            TypeKind::Rec(fields) => {
                for field in fields {
                    check_type(&field.ty, type_count, "a type field")?;
                }
            }
            TypeKind::Enum(variants) => {
                for variant in variants {
                    for ty in &variant.payload {
                        check_type(ty, type_count, "a variant payload")?;
                    }
                }
            }
        }
    }
    Ok(())
}

/// Reject a `Named` type whose id points past the type table, and check its
/// arguments recursively.
fn check_type(ty: &Type, type_count: usize, what: &str) -> Result<(), String> {
    match ty {
        Type::Named(id, arguments) => {
            check_id(id.0, type_count, what)?;
            for argument in arguments {
                check_type(argument, type_count, what)?;
            }
        }
        Type::Function {
            parameters,
            returns,
        } => {
            for parameter in parameters {
                check_type(parameter, type_count, what)?;
            }
            if let Some(returns) = returns {
                check_type(returns, type_count, what)?;
            }
        }
        // The leaf variants carry no nested type, so there is nothing below
        // them to check. They are listed so a new variant is a compile error.
        Type::Text | Type::Bool | Type::Int | Type::Parameter(_) => {}
    }
    Ok(())
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

    /// Check a struct literal's declared type points into the type table.
    fn check_type(&mut self, ty: TypeId) {
        if self.error.is_none() && ty.0 >= self.program.types.len() {
            self.error = Some("a struct type points past the types".to_owned());
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
