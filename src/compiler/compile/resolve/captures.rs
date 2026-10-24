//! Closure capture analysis.
//!
//! A function literal captures only the enclosing locals its body reads. A
//! nested literal is an anonymous function declaration of its own, so the
//! enclosing literal also carries the captures that nested function needs: a
//! value a nested body reads must be present in this frame too. A reference to
//! a local the literal itself declares is not a capture.

use crate::compiler::model::{
    Declaration, DeclarationId, Expression, MatchBody, Pattern, Statement, walk_statements,
};

use super::builder::Builder;

impl Builder {
    /// The enclosing locals a function literal body reads, plus the enclosing
    /// locals a nested literal's own captures name, so a value they need is
    /// available in this frame. A reference is a capture when it names a local
    /// of an enclosing function that this function does not declare.
    pub(in crate::compiler::compile::resolve) fn collect_captures(
        &self,
        body: &[Statement],
        parameters: &[DeclarationId],
    ) -> Vec<DeclarationId> {
        let mut locals = LocalCollector {
            locals: parameters.iter().copied().collect(),
        };
        walk_statements(&mut locals, body);
        let mut collector = CaptureCollector {
            declarations: &self.declarations,
            locals: &locals.locals,
            captures: Vec::new(),
        };
        walk_statements(&mut collector, body);
        collector.captures
    }
}

/// Collects the local declarations a function literal body introduces: its
/// parameters and its bindings. A reference to one of these is not a capture
/// of the literal.
struct LocalCollector {
    locals: std::collections::HashSet<DeclarationId>,
}

impl crate::compiler::model::Visitor for LocalCollector {
    fn function(&mut self, expression: &Expression) {
        let Expression::Function(function) = expression else {
            unreachable!("the walk names a function");
        };
        self.locals.extend(function.parameters.iter().copied());
    }

    fn bind(&mut self, statement: &Statement) {
        let Statement::Bind { declaration, .. } = statement else {
            unreachable!("the walk names a binding statement");
        };
        self.locals.insert(*declaration);
    }

    fn variant_pattern(&mut self, pattern: &Pattern) {
        let Pattern::Variant { bindings, .. } = pattern else {
            unreachable!("the walk names a variant pattern");
        };
        self.locals.extend(bindings.iter().flatten().copied());
    }

    fn text(&mut self, _: &Expression) {}
    fn map(&mut self, _: &Expression) {}
    fn boolean(&mut self, _: &Expression) {}
    fn integer(&mut self, _: &Expression) {}
    fn variant(&mut self, _: &Expression) {}
    fn interpolated(&mut self, _: &Expression) {}
    fn reference(&mut self, _: &Expression) {}
    fn call(&mut self, _: &Expression) {}
    fn method(&mut self, _: &Expression) {}
    fn field(&mut self, _: &Expression) {}
    fn binary(&mut self, _: &Expression) {}
    fn unary(&mut self, _: &Expression) {}
    fn match_expression(&mut self, _: &Expression) {}
    fn assign(&mut self, _: &Statement) {}
    fn field_assign(&mut self, _: &Statement) {}
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

/// Collects the enclosing locals a function literal body reads, and the
/// captures a nested literal declaration names, so a value they need is
/// present in this frame too.
struct CaptureCollector<'a> {
    declarations: &'a [Declaration],
    locals: &'a std::collections::HashSet<DeclarationId>,
    captures: Vec<DeclarationId>,
}

impl CaptureCollector<'_> {
    /// Record one enclosing local, unless this function declares it or a
    /// previous reference already named it.
    fn record(&mut self, declaration: DeclarationId) {
        let is_local = self.declarations[declaration.0].kind.local();
        if is_local && !self.locals.contains(&declaration) && !self.captures.contains(&declaration)
        {
            self.captures.push(declaration);
        }
    }
}

impl crate::compiler::model::Visitor for CaptureCollector<'_> {
    fn reference(&mut self, expression: &Expression) {
        let Expression::Reference { declaration, .. } = expression else {
            unreachable!("the walk names a reference");
        };
        let target = &self.declarations[declaration.0];
        if target.kind.local() {
            self.record(*declaration);
            return;
        }
        // A nested literal is no longer written inside this body: the body
        // holds a reference to its declaration. Carry the nested declaration's
        // captures into this frame, minus the locals this function already
        // declares.
        if target.is_anonymous_function() {
            let nested = target
                .function()
                .map_or_else(Vec::new, |function| function.captures.clone());
            for capture in nested {
                self.record(capture);
            }
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
    fn function(&mut self, _: &Expression) {}
    fn bind(&mut self, _: &Statement) {}
    fn assign(&mut self, _: &Statement) {}
    fn field_assign(&mut self, _: &Statement) {}
    fn expression_statement(&mut self, _: &Statement) {}
    fn return_statement(&mut self, _: &Statement) {}
    fn panic(&mut self, _: &Statement) {}
    fn asynchronous(&mut self, _: &Statement) {}
    fn wait(&mut self, _: &Statement) {}
    fn forever(&mut self, _: &Statement) {}
    fn break_statement(&mut self, _: &Statement) {}
    fn value_pattern(&mut self, _: &Pattern) {}
    fn variant_pattern(&mut self, _: &Pattern) {}
    fn expression_body(&mut self, _: &MatchBody) {}
    fn block_body(&mut self, _: &MatchBody) {}

    fn enters_deferred_body(&self) -> bool {
        true
    }
}
