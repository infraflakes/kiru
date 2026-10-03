//! Structural verification of the linked model.
//!
//! Linking runs this on its own output, and retention runs it again after
//! remapping every id; a failed assertion is a compiler bug.

use crate::compiler::{
    DeclarationId, DeclarationKind, Expression, Field, FileId, Program, Statement, Visitor,
    walk_expression, walk_statements,
};

pub(crate) fn verify(program: &Program) {
    let declaration_count = program.declarations.len();
    let file_count = program.files.len();
    let namespace_count = program.namespaces.len();

    for namespace in &program.namespaces {
        assert!(namespace.parent.is_some() || namespace.name.is_empty());
        if let Some(parent) = namespace.parent {
            assert!(parent.0 < namespace_count);
        }
        for child in namespace.children.values() {
            assert!(child.0 < namespace_count);
        }
        for declaration in namespace.values.iter().chain(namespace.functions.iter()) {
            assert!(declaration.0 < declaration_count);
        }
    }
    for file in &program.files {
        assert!(!file.path.as_os_str().is_empty());
        assert!(file.namespace.0 < namespace_count);
        for import in &file.imports {
            assert!(import.0 < file_count);
        }
        for declaration in &file.declarations {
            assert!(declaration.0 < declaration_count);
        }
        let _ = file.origin;
    }
    for (index, declaration) in program.declarations.iter().enumerate() {
        let id = DeclarationId(index);
        assert!(declaration.name_span.start <= declaration.name_span.end);
        if let Some(file) = declaration.file {
            assert!(file.0 < file_count);
        }
        assert!(declaration.namespace.0 < namespace_count);
        if let Some(owner) = declaration.owner {
            assert!(owner.0 < declaration_count);
        }
        for parameter in &declaration.parameters {
            assert!(parameter.0 < declaration_count);
            assert_eq!(program.declaration(*parameter).owner, Some(id));
        }
        match &declaration.kind {
            DeclarationKind::Function(function) => {
                verify_statements(program, &function.body);
            }
            DeclarationKind::Text(expression) | DeclarationKind::Record(expression) => {
                verify_expression(program, expression);
            }
            DeclarationKind::Binding(binding) => {
                let _ = binding;
            }
            DeclarationKind::Native(native) => {
                let _ = crate::compiler::native_row(*native);
            }
        }
        let _ = &declaration.name;
        let _ = declaration.order;
        let _ = declaration.derived.kind;
    }
    for value in program
        .values
        .read()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
        .keys()
    {
        assert!(value.0 < declaration_count);
    }
    assert!(program.entry.0 < declaration_count);
    assert!(matches!(
        program.declaration(program.entry).kind,
        DeclarationKind::Function(_)
    ));
    assert_eq!(program.declaration(program.entry).namespace, program.root());
    assert!(program.namespace(program.root()).parent.is_none());
    assert!(program.namespace_at(&[]).is_some());
    let _ = program.namespace_path(program.root());
    for file in 0..file_count {
        let _ = program.file(FileId(file));
    }
}

/// The structural checks over one body: every span is ordered, every edge
/// points at a declaration, and every native row exists.
struct StructuralCheck<'a> {
    program: &'a Program,
}

impl Visitor for StructuralCheck<'_> {
    fn expression(&mut self, expression: &Expression) {
        let span = expression.span();
        assert!(span.start <= span.end);
        match expression {
            Expression::Text { value, .. } => {
                let _ = value;
            }
            Expression::Record { fields, .. } => assert_field_spans(fields),
            Expression::Reference { declaration, .. } => {
                assert!(declaration.0 < self.program.declarations.len());
            }
            Expression::Call {
                callee,
                callee_span,
                ..
            } => {
                assert!(callee.0 < self.program.declarations.len());
                assert!(callee_span.start <= callee_span.end);
            }
            Expression::Field {
                name, name_span, ..
            } => {
                assert!(name_span.start <= name_span.end);
                let _ = name;
            }
            Expression::Add { .. } => {}
        }
    }

    fn statement(&mut self, statement: &Statement) {
        match statement {
            Statement::Bind {
                declaration, span, ..
            } => {
                assert!(declaration.0 < self.program.declarations.len());
                assert!(span.start <= span.end);
            }
            Statement::Assign {
                declaration,
                name_span,
                span,
                ..
            } => {
                assert!(declaration.0 < self.program.declarations.len());
                assert!(name_span.start <= name_span.end);
                assert!(span.start <= span.end);
            }
            Statement::Expression(_) => {}
            Statement::Return { span, .. } => assert!(span.start <= span.end),
            Statement::Panic { span } => assert!(span.start <= span.end),
            Statement::Async { span, .. } => assert!(span.start <= span.end),
            Statement::Wait { span } => assert!(span.start <= span.end),
            Statement::Switch { span, cases, .. } => {
                assert!(span.start <= span.end);
                for case in cases {
                    assert!(case.span.start <= case.span.end);
                }
            }
            Statement::Defer { span, .. } => assert!(span.start <= span.end),
        }
    }
}

/// The span checks shared by a record expression and a record declaration.
fn assert_field_spans(fields: &[Field]) {
    for field in fields {
        assert!(field.name_span.start <= field.name_span.end);
        assert!(field.span.start <= field.span.end);
        let _ = &field.name;
    }
}

fn verify_statements(program: &Program, statements: &[Statement]) {
    let mut check = StructuralCheck { program };
    walk_statements(&mut check, statements);
}

fn verify_expression(program: &Program, expression: &Expression) {
    let mut check = StructuralCheck { program };
    walk_expression(&mut check, expression);
}
