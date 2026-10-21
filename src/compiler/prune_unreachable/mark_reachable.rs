//! Reachability: the declarations the entry can reach.

use std::collections::HashSet;

use crate::compiler::model::{
    DeclarationId, Expression, MatchBody, Pattern, Program, Statement, Visitor, walk_expression,
};

/// The declarations reachable from the entry: every function's parameters and
/// the bindings, references, and callees in the bodies and initializers it
/// reaches. Visited nodes are recorded, so a shared callee is walked once.
pub(super) fn collect_reachable_declarations(program: &Program) -> HashSet<DeclarationId> {
    let mut reachable = HashSet::new();
    let mut collector = ReachableCollector {
        pending: vec![program.entry.expect("a compilation prunes from its entry")],
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

impl ReachableCollector {
    /// The declaration a binding statement declares.
    fn declared(&mut self, statement: &Statement) {
        let (Statement::Bind { declaration, .. }
        | Statement::Assign { declaration, .. }
        | Statement::FieldAssign { declaration, .. }) = statement
        else {
            unreachable!("the walk names a binding statement");
        };
        self.pending.push(*declaration);
    }
}

impl Visitor for ReachableCollector {
    fn reference(&mut self, expression: &Expression) {
        let Expression::Reference { declaration, .. } = expression else {
            unreachable!("the walk names a reference");
        };
        self.pending.push(*declaration);
    }

    // A function value owns its parameters; an unused one still needs a
    // register, so every parameter is retained.
    fn function(&mut self, expression: &Expression) {
        let Expression::Function(function) = expression else {
            unreachable!("the walk names a function");
        };
        self.pending.extend(function.parameters.iter().copied());
    }

    fn bind(&mut self, statement: &Statement) {
        self.declared(statement);
    }
    fn assign(&mut self, statement: &Statement) {
        self.declared(statement);
    }
    fn field_assign(&mut self, statement: &Statement) {
        self.declared(statement);
    }

    fn variant_pattern(&mut self, pattern: &Pattern) {
        let Pattern::Variant { bindings, .. } = pattern else {
            unreachable!("the walk names a variant pattern");
        };
        for id in bindings.iter().flatten() {
            self.pending.push(*id);
        }
    }

    fn text(&mut self, _: &Expression) {}
    fn map(&mut self, _: &Expression) {}
    fn boolean(&mut self, _: &Expression) {}
    fn integer(&mut self, _: &Expression) {}
    fn variant(&mut self, _: &Expression) {}
    fn interpolated(&mut self, _: &Expression) {}
    fn call(&mut self, _: &Expression) {}
    fn method(&mut self, _: &Expression) {}
    fn field(&mut self, _: &Expression) {}
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
