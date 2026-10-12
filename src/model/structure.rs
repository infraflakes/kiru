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
    // the root namespace's `main` function; only its id is checked here.
    check_id(program.entry.0, declaration_count, "the entry")?;
    if program.namespace(program.root()).parent.is_some() {
        return Err("the root namespace has a parent".to_owned());
    }
    if program.namespace_at(&[]).is_none() {
        return Err("the root namespace is missing".to_owned());
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
                check_statements(program, &function.body, declaration.order)?;
            }
            Some(value) => {
                check_expression(program, value, declaration.order)?;
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
/// arguments (and a Vec's element) recursively.
fn check_type(ty: &Type, type_count: usize, what: &str) -> Result<(), String> {
    match ty {
        Type::Named(id, arguments) => {
            check_id(id.0, type_count, what)?;
            for argument in arguments {
                check_type(argument, type_count, what)?;
            }
        }
        Type::Vec(element) => check_type(element, type_count, what)?,
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
fn check_statements(
    program: &Program,
    statements: &[Statement],
    holder_order: usize,
) -> Result<(), String> {
    let mut check = EdgeCheck {
        program,
        holder_order,
        error: None,
    };
    walk_statements(&mut check, statements);
    check.error.map_or(Ok(()), Err)
}

fn check_expression(
    program: &Program,
    expression: &Expression,
    holder_order: usize,
) -> Result<(), String> {
    let mut check = EdgeCheck {
        program,
        holder_order,
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
        if let Statement::Assign { name_span, .. }
        | Statement::FieldAssign { name_span, .. }
        | Statement::IndexAssign { name_span, .. } = statement
        {
            assert!(name_span.start <= name_span.end);
        }
    }
}

/// Checks every reference and call edge points at a declaration, and that the
/// top-down rule holds: a call names a declaration made strictly earlier, and
/// a reference names one made no later. A local binding and a parameter share
/// their function's order, so a reference to one is no later by definition.
/// This is what makes a decoded graph terminate: every call chain strictly
/// decreases in order. Every node kind is listed, so a new edge-bearing kind
/// fails to compile.
struct EdgeCheck<'a> {
    program: &'a Program,
    holder_order: usize,
    error: Option<String>,
}

impl EdgeCheck<'_> {
    fn check(&mut self, declaration: DeclarationId, what: &str) {
        if self.error.is_none() && declaration.0 >= self.program.declarations.len() {
            self.error = Some(format!("{what} points past the declarations"));
        }
    }

    /// Check a call edge: the callee must be strictly earlier.
    fn check_call(&mut self, callee: DeclarationId) {
        self.check(callee, "a call");
        self.check_order(callee, "a call", false);
    }

    /// Check a reference edge: the target must be no later.
    fn check_reference(&mut self, target: DeclarationId) {
        self.check(target, "a reference");
        self.check_order(target, "a reference", true);
    }

    /// Check a struct literal's declared type points into the type table.
    fn check_type(&mut self, ty: TypeId) {
        if self.error.is_none() && ty.0 >= self.program.types.len() {
            self.error = Some("a struct type points past the types".to_owned());
        }
    }

    fn check_order(&mut self, target: DeclarationId, what: &str, allow_equal: bool) {
        if self.error.is_some() {
            return;
        }
        let target_order = self.program.declaration(target).order;
        let ordered = if allow_equal {
            target_order <= self.holder_order
        } else {
            target_order < self.holder_order
        };
        if !ordered {
            self.error = Some(format!(
                "{what} points at a declaration that is not earlier"
            ));
        }
    }
}

impl Visitor for EdgeCheck<'_> {
    fn expression(&mut self, expression: &Expression) {
        match expression {
            Expression::Reference { declaration, .. } => self.check_reference(*declaration),
            Expression::Call { callee, .. } => {
                // A direct call names a callable declaration; a call through a
                // value is a reference like any other.
                if let Expression::Reference { declaration, .. } = callee.as_ref()
                    && self.program.declaration(*declaration).is_callable()
                {
                    self.check_call(*declaration);
                }
            }
            Expression::Variant { ty, .. } => self.check_type(*ty),
            // A function literal owns its parameters and reads its captures.
            Expression::Function(function) => {
                for parameter in &function.parameters {
                    self.check(*parameter, "a function parameter");
                }
                for capture in &function.captures {
                    self.check_reference(*capture);
                }
            }
            Expression::Text { .. }
            | Expression::Map { .. }
            | Expression::Vec { .. }
            | Expression::Bool { .. }
            | Expression::Int { .. }
            | Expression::Field { .. }
            | Expression::Index { .. }
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
            | Statement::FieldAssign { declaration, .. }
            | Statement::IndexAssign { declaration, .. } => {
                self.check(*declaration, "a statement binding");
            }
            Statement::ForEach { item, .. } => {
                self.check(*item, "a loop binding");
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compiler::checked_program;

    #[test]
    fn rejects_a_call_that_is_not_earlier() {
        let mut program = checked_program("fn a() {};\nfn b() { a(); };\nfn main() {};");
        let b = program
            .namespace_at(&[])
            .and_then(|root| program.namespace(root).get("b").first().copied());
        let b = b.expect("b is declared");
        let declaration = program.declaration_mut(b);
        let Some(Expression::Function(function)) = &mut declaration.value else {
            panic!("b is a function");
        };
        let span = function.body[0].span();
        let Statement::Expression(Expression::Call { callee, .. }) = &mut function.body[0] else {
            panic!("b calls a");
        };
        **callee = Expression::Reference {
            declaration: b,
            span,
        };
        let violation = validate_program_structure(&program).expect_err("a self-call is rejected");
        assert!(violation.contains("not earlier"), "{violation}");
    }
}
