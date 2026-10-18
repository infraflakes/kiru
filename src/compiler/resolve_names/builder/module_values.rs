//! Ordering the module values by their initializer dependencies.
//!
//! A value's initializer is evaluated before the values that read it. Only a
//! reference in the initializer itself counts, and only another module value in
//! the same namespace is a dependency — a module value is never `pub`, so no
//! value crosses a namespace. A cycle among them is an error.

use crate::compiler::Diagnostic;
use crate::compiler::model::{
    DeclarationId, Expression, MatchBody, Pattern, Statement, Visitor, walk_expression,
};

use super::registration::Builder;

impl Builder {
    /// The module values in dependency order: a value's initializer is
    /// evaluated before the values that read it. Only a reference in the
    /// initializer itself counts, and only another module value in the same
    /// namespace is a dependency — a module value is never `pub`, so no value
    /// crosses a namespace. A cycle among them is an error.
    pub(in crate::compiler::resolve_names) fn order_module_values(
        &self,
    ) -> Result<Vec<DeclarationId>, Diagnostic> {
        let count = self.declarations.len();
        let mut deps: Vec<Vec<DeclarationId>> = vec![Vec::new(); count];
        for (index, dependencies) in deps.iter_mut().enumerate() {
            let id = DeclarationId(index);
            if !self.is_module_value(id) {
                continue;
            }
            let declaration = self.declarations[index]
                .as_ref()
                .expect("every node is built during linking");
            let namespace = declaration.namespace;
            let mut collector = InitializerReferences {
                references: Vec::new(),
            };
            walk_expression(
                &mut collector,
                declaration
                    .value
                    .as_ref()
                    .expect("a module value has an initializer"),
            );
            for reference in collector.references {
                if self.is_module_value(reference)
                    && self.declarations[reference.0]
                        .as_ref()
                        .expect("every node is built during linking")
                        .namespace
                        == namespace
                    && !dependencies.contains(&reference)
                {
                    dependencies.push(reference);
                }
            }
        }
        let mut order = Vec::new();
        // 0 unvisited, 1 on the current path, 2 finished.
        let mut state = vec![0u8; count];
        for index in 0..count {
            let id = DeclarationId(index);
            if self.is_module_value(id) {
                self.visit_module_value(id, &deps, &mut state, &mut order)?;
            }
        }
        Ok(order)
    }

    /// Whether a declaration is a module value: a top-level `let`, not a local
    /// binding. A function is code, not data, so it is evaluated when called.
    fn is_module_value(&self, id: DeclarationId) -> bool {
        self.declarations[id.0]
            .as_ref()
            .is_some_and(|declaration| declaration.kind.has_value() && declaration.owner.is_none())
    }

    /// Depth-first visit of one module value, emitting it after its
    /// dependencies. A value reached while already on the path closes a cycle.
    fn visit_module_value(
        &self,
        id: DeclarationId,
        deps: &[Vec<DeclarationId>],
        state: &mut [u8],
        order: &mut Vec<DeclarationId>,
    ) -> Result<(), Diagnostic> {
        match state[id.0] {
            2 => return Ok(()),
            1 => {
                let declaration = self.declarations[id.0]
                    .as_ref()
                    .expect("every node is built during linking");
                let file = declaration
                    .file
                    .expect("a module value is a file declaration, so it has a file");
                return Err(Diagnostic::new(
                    &self.files[file.0].path,
                    self.skeletons[id.0].name_span,
                    format!(
                        "`{}` is part of a cycle among module values",
                        declaration.name
                    ),
                ));
            }
            _ => {}
        }
        state[id.0] = 1;
        for dependency in &deps[id.0] {
            self.visit_module_value(*dependency, deps, state, order)?;
        }
        state[id.0] = 2;
        order.push(id);
        Ok(())
    }
}

/// Collects the declarations an initializer references.
struct InitializerReferences {
    references: Vec<DeclarationId>,
}

impl Visitor for InitializerReferences {
    fn reference(&mut self, expression: &Expression) {
        let Expression::Reference { declaration, .. } = expression else {
            unreachable!("the walk names a reference");
        };
        self.references.push(*declaration);
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
}
