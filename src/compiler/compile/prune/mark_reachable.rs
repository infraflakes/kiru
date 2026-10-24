//! Reachability: the declarations the entry can reach.

use std::collections::HashSet;

use crate::compiler::model::{
    DeclarationId, Expression, Facts, MatchBody, Pattern, Program, Site, Statement, Visitor,
    walk_expression,
};

/// The declarations reachable from the entry: every function's parameters and
/// the bindings, references, and callees in the bodies and initializers it
/// reaches. A method family's callee is the target the check recorded, read
/// from `Facts`. Visited nodes are recorded, so a shared callee is walked once.
pub(crate) fn mark_reachable(
    program: &Program,
    facts: &Facts,
    entry: DeclarationId,
) -> HashSet<DeclarationId> {
    let mut reachable = HashSet::new();
    let mut collector = ReachableCollector {
        pending: vec![entry],
        facts,
        owner: entry,
    };
    while let Some(id) = collector.pending.pop() {
        if !reachable.insert(id) {
            continue;
        }
        collector.owner = id;
        let declaration = program.declaration(id);
        if let Some(value) = declaration.value() {
            walk_expression(&mut collector, value);
        }
    }
    reachable
}

/// Collects the declaration ids a body points at: the binding a statement
/// declares, the declaration a reference names, and the target of a call's
/// family. Every node kind is listed, so a new edge-bearing kind fails to
/// compile.
struct ReachableCollector<'f> {
    pending: Vec<DeclarationId>,
    facts: &'f Facts,
    /// The declaration whose body is being walked.
    owner: DeclarationId,
}

impl ReachableCollector<'_> {
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

impl Visitor for ReachableCollector<'_> {
    fn reference(&mut self, expression: &Expression) {
        let Expression::Reference { declaration, .. } = expression else {
            unreachable!("the walk names a reference");
        };
        self.pending.push(*declaration);
    }

    // A family carries no graph edge; the check recorded the target selection
    // in `Facts`, and reachability follows that fact.
    fn method(&mut self, expression: &Expression) {
        let Expression::Method { span, .. } = expression else {
            unreachable!("the walk names a method family");
        };
        if let Some(target) = self.facts.method_target(Site::new(self.owner, *span)) {
            self.pending.push(target);
        }
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
