//! Structural verification of the program model.
//!
//! `validate_program_structure` checks every id and edge against the tables it
//! points into and reports the first violation; it runs on a program decoded
//! from an executable, where the bytes are untrusted. `verify_program` runs
//! the same check as an internal assertion plus the span checks, so a compiler
//! that produces an invalid graph fails loudly during development.

use super::callable_registry::native_row;
use super::lock::RwLockExt;
use super::nodes::{DeclarationId, DeclarationKind, Expression, Field, Program, Statement};
use super::traverse_program::{Visitor, walk_expression, walk_statements};

/// Assert the program is structurally valid. A violation is a compiler bug.
pub(crate) fn verify_program(program: &Program) {
    if let Err(violation) = validate_program_structure(program) {
        panic!("program structure is invalid: {violation}");
    }
    verify_spans(program);
}

/// Check every id and edge in the program, returning the first violation. A
/// program decoded from an executable is validated with this before it runs.
pub(crate) fn validate_program_structure(program: &Program) -> Result<(), String> {
    let declaration_count = program.declarations.len();
    let file_count = program.files.len();
    let namespace_count = program.namespaces.len();

    check_id(program.entry.0, declaration_count, "the entry")?;
    if !matches!(
        program.declaration(program.entry).kind,
        DeclarationKind::Function(_)
    ) {
        return Err("the entry is not a function".to_owned());
    }
    if program.declaration(program.entry).namespace != program.root() {
        return Err("the entry is not in the root namespace".to_owned());
    }
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
        for declaration in namespace.values.iter().chain(namespace.functions.iter()) {
            check_id(declaration.0, declaration_count, "a registered declaration")?;
        }
    }
    for file in &program.files {
        check_id(file.namespace.0, namespace_count, "a file namespace")?;
        for import in &file.imports {
            check_id(import.0, file_count, "a file import")?;
        }
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
        for parameter in &declaration.parameters {
            check_id(parameter.0, declaration_count, "a declaration parameter")?;
        }
        match &declaration.kind {
            DeclarationKind::Function(function) => {
                check_statements(&function.body, declaration_count)?;
            }
            DeclarationKind::Text(expression) | DeclarationKind::Record(expression) => {
                check_expression(expression, declaration_count)?;
            }
            DeclarationKind::Binding(_) => {}
            DeclarationKind::Native(native) => {
                native_row(*native);
            }
        }
    }
    for value in program.values.read_unpoisoned().keys() {
        check_id(value.0, declaration_count, "a module value")?;
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
        match &declaration.kind {
            DeclarationKind::Function(function) => {
                walk_statements(&mut check, &function.body);
            }
            DeclarationKind::Text(expression) | DeclarationKind::Record(expression) => {
                walk_expression(&mut check, expression);
            }
            DeclarationKind::Binding(_) | DeclarationKind::Native(_) => {}
        }
    }
}

/// Walk a body and check every reference and call edge against the model.
fn check_statements(statements: &[Statement], declaration_count: usize) -> Result<(), String> {
    let mut check = EdgeCheck {
        declaration_count,
        error: None,
    };
    walk_statements(&mut check, statements);
    check.error.map_or(Ok(()), Err)
}

fn check_expression(expression: &Expression, declaration_count: usize) -> Result<(), String> {
    let mut check = EdgeCheck {
        declaration_count,
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
        if let Expression::Record { fields, .. } = expression {
            assert_field_spans(fields);
        }
    }

    fn statement(&mut self, statement: &Statement) {
        match statement {
            Statement::Bind { span, .. } => assert!(span.start <= span.end),
            Statement::Assign {
                name_span, span, ..
            } => {
                assert!(name_span.start <= name_span.end);
                assert!(span.start <= span.end);
            }
            Statement::Expression(_) => {}
            Statement::Return { span, .. }
            | Statement::Panic { span }
            | Statement::Async { span, .. }
            | Statement::Wait { span }
            | Statement::Defer { span, .. } => assert!(span.start <= span.end),
            Statement::Switch { span, cases, .. } => {
                assert!(span.start <= span.end);
                for case in cases {
                    assert!(case.span.start <= case.span.end);
                }
            }
        }
    }
}

/// Checks every reference and call edge points at a declaration. Every node
/// kind is listed, so a new edge-bearing kind fails to compile.
struct EdgeCheck {
    declaration_count: usize,
    error: Option<String>,
}

impl EdgeCheck {
    fn check(&mut self, declaration: DeclarationId, what: &str) {
        if self.error.is_none() && declaration.0 >= self.declaration_count {
            self.error = Some(format!("{what} points past the declarations"));
        }
    }
}

impl Visitor for EdgeCheck {
    fn expression(&mut self, expression: &Expression) {
        match expression {
            Expression::Reference { declaration, .. } => self.check(*declaration, "a reference"),
            Expression::Call { callee, .. } => self.check(*callee, "a call"),
            Expression::Text { .. }
            | Expression::Record { .. }
            | Expression::Field { .. }
            | Expression::Add { .. } => {}
        }
    }

    fn statement(&mut self, statement: &Statement) {
        match statement {
            Statement::Bind { declaration, .. } | Statement::Assign { declaration, .. } => {
                self.check(*declaration, "a statement binding");
            }
            Statement::Expression(_)
            | Statement::Return { .. }
            | Statement::Panic { .. }
            | Statement::Async { .. }
            | Statement::Wait { .. }
            | Statement::Switch { .. }
            | Statement::Defer { .. } => {}
        }
    }
}

/// The span checks shared by a record expression and a record declaration.
fn assert_field_spans(fields: &[Field]) {
    for field in fields {
        assert!(field.name_span.start <= field.name_span.end);
        assert!(field.span.start <= field.span.end);
    }
}
