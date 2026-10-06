//! Structural verification of the program model.
//!
//! `validate_program_structure` checks every id and edge against the tables it
//! points into, reports the first violation, and enforces the top-down rule on
//! every edge. It is a compile-time invariant: `verify_program` runs it plus
//! the span checks, so a compiler that produces an invalid graph fails loudly
//! during development. The untrusted decode boundary is
//! `validate_bytecode_structure`, which runs on the lowered bytecode.

use crate::native_registry::native_row;

use super::declarations::DeclarationKind;
use super::expressions::{Expression, Field, Statement};
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
                check_statements(program, &function.body, declaration.order)?;
                if declaration.derived.kind != Some(function.return_kind) {
                    return Err("a function's kind does not match its declaration".to_owned());
                }
            }
            DeclarationKind::Text(expression)
            | DeclarationKind::Record(expression)
            | DeclarationKind::List(expression) => {
                check_expression(program, expression, declaration.order)?;
            }
            DeclarationKind::Binding(_) => {}
            DeclarationKind::Native(native) => {
                native_row(*native);
            }
        }
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
            DeclarationKind::Text(expression)
            | DeclarationKind::Record(expression)
            | DeclarationKind::List(expression) => {
                walk_expression(&mut check, expression);
            }
            DeclarationKind::Binding(_) | DeclarationKind::Native(_) => {}
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
        if let Expression::Record { fields, .. } = expression {
            assert_field_spans(fields);
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
        if let Statement::Switch { cases, .. } = statement {
            for case in cases {
                assert!(case.span.start <= case.span.end);
            }
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
            Expression::Call { callee, .. } => self.check_call(*callee),
            Expression::Text { .. }
            | Expression::Record { .. }
            | Expression::List { .. }
            | Expression::Field { .. }
            | Expression::Add { .. } => {}
        }
    }

    fn statement(&mut self, statement: &Statement) {
        match statement {
            Statement::Bind { declaration, .. }
            | Statement::Assign { declaration, .. }
            | Statement::FieldAssign { declaration, .. } => {
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
            | Statement::Switch { .. }
            | Statement::Break { .. }
            | Statement::Forever { .. } => {}
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compiler::checked_program;

    #[test]
    fn rejects_a_call_that_is_not_earlier() {
        let mut program = checked_program("fn a() {};\nfn b() { a(); };\nfn main() {};");
        let b = program
            .namespace_at(&[])
            .and_then(|root| program.namespace(root).function("b"));
        let b = b.expect("b is declared");
        let function = program
            .declaration_mut(b)
            .function_mut()
            .expect("b is a function");
        let Statement::Expression(Expression::Call { callee, .. }) = &mut function.body[0] else {
            panic!("b calls a");
        };
        *callee = b;
        let violation = validate_program_structure(&program).expect_err("a self-call is rejected");
        assert!(violation.contains("not earlier"), "{violation}");
    }
}
