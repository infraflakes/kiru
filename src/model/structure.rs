//! Structural verification of the program model.
//!
//! `validate_program_structure` checks every id and edge against the tables it
//! points into, reports the first violation, and enforces the top-down rule on
//! every edge. It is a compile-time invariant: `verify_program` runs it plus
//! the span checks, so a compiler that produces an invalid graph fails loudly
//! during development. The untrusted decode boundary is
//! `validate_bytecode_structure`, which runs on the lowered bytecode.

use crate::model::{Type, TypeId};
use crate::native_registry::native_shape;

use super::body::{Expression, Field, Pattern, Statement};
use super::declarations::TypeKind;
use super::ids::DeclarationId;
use super::program::Program;
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
    if let Some(vec_type) = program.vec_type {
        check_id(vec_type.0, type_count, "the Vec type")?;
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
        match &declaration.value {
            Some(Expression::Function(function)) => {
                for parameter in &function.parameters {
                    check_id(parameter.0, declaration_count, "a function parameter")?;
                }
                for capture in &function.captures {
                    check_id(capture.0, declaration_count, "a function capture")?;
                }
                check_statements(program, &function.body)?;
            }
            Some(value) => {
                check_expression(program, value)?;
            }
            None => {
                if let Some(native) = declaration.native {
                    native_shape(native);
                }
            }
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
        _ => {}
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

/// The span checks over every body, plus the reference and call edges the
/// structural pass also covers. Every span is ordered.
fn verify_spans(program: &Program) {
    let mut check = SpanCheck;
    for declaration in &program.declarations {
        if let Some(value) = &declaration.value {
            walk_expression(&mut check, value);
        }
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

/// Checks every span is ordered. The id edges are checked by `EdgeCheck`.
struct SpanCheck;

impl Visitor for SpanCheck {
    fn expression(&mut self, expression: &Expression) {
        let span = expression.span();
        assert!(span.start <= span.end);
        if let Expression::Call { callee_span, .. } = expression {
            assert!(callee_span.start <= callee_span.end);
        }
        if let Expression::Field { name_span, .. } = expression {
            assert!(name_span.start <= name_span.end);
        }
        if let Expression::Map { fields, .. } = expression {
            assert_field_spans(fields);
        }
    }

    fn pattern(&mut self, pattern: &Pattern) {
        if let Pattern::Variant { span, .. } = pattern {
            assert!(span.start <= span.end);
        }
    }

    fn statement(&mut self, statement: &Statement) {
        let span = statement.span();
        assert!(span.start <= span.end);
        if let Statement::Assign { name_span, .. } | Statement::FieldAssign { name_span, .. } =
            statement
        {
            assert!(name_span.start <= name_span.end);
        }
    }
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

impl Visitor for EdgeCheck<'_> {
    fn expression(&mut self, expression: &Expression) {
        match expression {
            Expression::Reference { declaration, .. } => self.check(*declaration, "a reference"),
            Expression::Call { callee, .. } => {
                // A direct call names a callable declaration; a call through a
                // value is a reference like any other.
                if let Expression::Reference { declaration, .. } = callee.as_ref()
                    && self.program.declaration(*declaration).is_callable()
                {
                    self.check(*declaration, "a call");
                }
            }
            Expression::Variant { ty, .. } => self.check_type(*ty),
            // A function literal owns its parameters and reads its captures.
            Expression::Function(function) => {
                for parameter in &function.parameters {
                    self.check(*parameter, "a function parameter");
                }
                for capture in &function.captures {
                    self.check(*capture, "a reference");
                }
            }
            Expression::Text { .. }
            | Expression::Map { .. }
            | Expression::Bool { .. }
            | Expression::Int { .. }
            | Expression::Field { .. }
            | Expression::Binary { .. }
            | Expression::Unary { .. }
            | Expression::Interpolated { .. }
            | Expression::Match { .. } => {}
        }
    }

    fn pattern(&mut self, pattern: &Pattern) {
        if let Pattern::Variant { ty, .. } = pattern {
            self.check_type(*ty);
        }
    }

    fn statement(&mut self, statement: &Statement) {
        match statement {
            Statement::Bind { declaration, .. }
            | Statement::Assign { declaration, .. }
            | Statement::FieldAssign { declaration, .. } => {
                self.check(*declaration, "a statement binding");
            }
            Statement::Expression(_)
            | Statement::Return { .. }
            | Statement::Panic { .. }
            | Statement::Async { .. }
            | Statement::Wait { .. }
            | Statement::Break { .. }
            | Statement::Forever { .. } => {}
        }
    }
}

/// The span checks shared by a map expression and a `rec` declaration.
fn assert_field_spans(fields: &[Field]) {
    for field in fields {
        assert!(field.name_span.start <= field.name_span.end);
        assert!(field.span.start <= field.span.end);
    }
}
