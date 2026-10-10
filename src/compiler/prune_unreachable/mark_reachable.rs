//! Reachability: the declarations the entry can reach.

use std::collections::HashSet;

use crate::model::{
    DeclarationId, Expression, Program, Statement, Visitor, walk_expression, walk_statements,
};

/// The declarations reachable from the entry: every function's parameters and
/// the bindings, references, and callees in the bodies and initializers it
/// reaches. Visited nodes are recorded, so a shared callee is walked once.
pub(super) fn collect_reachable_declarations(program: &Program) -> HashSet<DeclarationId> {
    let mut reachable = HashSet::new();
    let mut collector = ReachableCollector {
        pending: vec![program.entry],
    };
    while let Some(id) = collector.pending.pop() {
        if !reachable.insert(id) {
            continue;
        }
        let declaration = program.declaration(id);
        collector
            .pending
            .extend(declaration.parameters.iter().copied());
        if let Some(function) = declaration.function() {
            walk_statements(&mut collector, &function.body);
        } else if let Some(expression) = declaration.initializer() {
            walk_expression(&mut collector, expression);
        }
    }
    reachable
}

/// Collects the declaration ids a body points at: the binding a statement
/// declares, the declaration a reference names, and the callee a call names.
/// Every node kind is listed, so a new edge-bearing kind fails to compile.
struct ReachableCollector {
    pending: Vec<DeclarationId>,
}

impl Visitor for ReachableCollector {
    fn expression(&mut self, expression: &Expression) {
        match expression {
            Expression::Reference { declaration, .. } => self.pending.push(*declaration),
            Expression::Call { callee, .. } => self.pending.push(*callee),
            Expression::Text { .. }
            | Expression::Record { .. }
            | Expression::List { .. }
            | Expression::Field { .. }
            | Expression::Add { .. }
            | Expression::Interpolated { .. }
            | Expression::Match { .. } => {}
        }
    }

    fn statement(&mut self, statement: &Statement) {
        match statement {
            Statement::Bind { declaration, .. }
            | Statement::Assign { declaration, .. }
            | Statement::FieldAssign { declaration, .. } => {
                self.pending.push(*declaration);
            }
            Statement::ForEach { item, .. } => self.pending.push(*item),
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
