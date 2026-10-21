//! Ordering the module values by their initializer dependencies.
//!
//! A value's initializer is evaluated before the values that read it. Only a
//! reference in the initializer itself counts, and only another module value in
//! the same namespace is a dependency — a module value is never `pub`, so no
//! value crosses a namespace. A cycle among them is an error.

use std::collections::HashMap;

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
    ///
    /// Registration records every module value, so this walks only that list:
    /// the declaration table is never scanned for values that are not there.
    pub(in crate::compiler::compile::resolve) fn order_module_values(
        &self,
        module_values: &[DeclarationId],
    ) -> Result<Vec<DeclarationId>, Diagnostic> {
        let positions: HashMap<DeclarationId, usize> = module_values
            .iter()
            .enumerate()
            .map(|(position, id)| (*id, position))
            .collect();
        let mut deps: Vec<Vec<usize>> = Vec::with_capacity(module_values.len());
        for id in module_values {
            let declaration = &self.declarations[id.0];
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
            let mut dependencies = Vec::new();
            for reference in collector.references {
                let Some(&position) = positions.get(&reference) else {
                    continue;
                };
                if self.declarations[reference.0].namespace == namespace
                    && !dependencies.contains(&position)
                {
                    dependencies.push(position);
                }
            }
            deps.push(dependencies);
        }
        let mut order = Vec::with_capacity(module_values.len());
        // 0 unvisited, 1 on the current path, 2 finished.
        let mut state = vec![0u8; module_values.len()];
        for position in 0..module_values.len() {
            self.visit_module_value(position, module_values, &deps, &mut state, &mut order)?;
        }
        Ok(order)
    }

    /// Depth-first visit of one module value, emitting it after its
    /// dependencies. A value reached while already on the path closes a cycle.
    fn visit_module_value(
        &self,
        position: usize,
        module_values: &[DeclarationId],
        deps: &[Vec<usize>],
        state: &mut [u8],
        order: &mut Vec<DeclarationId>,
    ) -> Result<(), Diagnostic> {
        match state[position] {
            2 => return Ok(()),
            1 => {
                let id = module_values[position];
                let declaration = &self.declarations[id.0];
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
        state[position] = 1;
        for &dependency in &deps[position] {
            self.visit_module_value(dependency, module_values, deps, state, order)?;
        }
        state[position] = 2;
        order.push(module_values[position]);
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

    // An initializer is evaluated when the module value is made. A function
    // literal's body runs only when the value is called, so its references are
    // not dependencies of the initializer.
    fn enters_deferred_body(&self) -> bool {
        false
    }
}
