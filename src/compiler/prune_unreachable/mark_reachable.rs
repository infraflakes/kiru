//! Reachability: the declarations the entry can reach.

use std::collections::HashSet;

use crate::model::{
    DeclarationId, Expression, Pattern, Program, Statement, Visitor, walk_expression,
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
        if let Some(value) = declaration.value() {
            walk_expression(&mut collector, value);
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
            // A function value owns its parameters; an unused one still needs
            // a register, so every parameter is retained.
            Expression::Function(function) => {
                self.pending.extend(function.parameters.iter().copied());
            }
            Expression::Text { .. }
            | Expression::Map { .. }
            | Expression::Vec { .. }
            | Expression::Bool { .. }
            | Expression::Int { .. }
            | Expression::Variant { .. }
            | Expression::Call { .. }
            | Expression::Field { .. }
            | Expression::Index { .. }
            | Expression::Binary { .. }
            | Expression::Unary { .. }
            | Expression::Interpolated { .. }
            | Expression::Match { .. } => {}
        }
    }

    fn pattern(&mut self, pattern: &Pattern) {
        if let Pattern::Variant { bindings, .. } = pattern {
            for id in bindings.iter().flatten() {
                self.pending.push(*id);
            }
        }
    }

    fn statement(&mut self, statement: &Statement) {
        match statement {
            Statement::Bind { declaration, .. }
            | Statement::Assign { declaration, .. }
            | Statement::FieldAssign { declaration, .. }
            | Statement::IndexAssign { declaration, .. } => {
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
