//! Reachability: the declarations the entry can reach.

use std::collections::HashSet;

use crate::compiler::{
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
struct ReachableCollector {
    pending: Vec<DeclarationId>,
}

impl Visitor for ReachableCollector {
    fn expression(&mut self, expression: &Expression) {
        match expression {
            Expression::Reference { declaration, .. } => self.pending.push(*declaration),
            Expression::Call { callee, .. } => self.pending.push(*callee),
            _ => {}
        }
    }

    fn statement(&mut self, statement: &Statement) {
        match statement {
            Statement::Bind { declaration, .. } | Statement::Assign { declaration, .. } => {
                self.pending.push(*declaration)
            }
            _ => {}
        }
    }
}
