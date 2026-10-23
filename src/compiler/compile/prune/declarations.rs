//! Compaction and remapping: rewrite the model to the reachable set.

use std::collections::HashSet;

use crate::compiler::model::{
    Declaration, DeclarationId, Expression, MatchBody, Pattern, Program, Statement, VisitorMut,
    walk_expression_mut,
};

/// Keep only the declarations the entry reaches, remapping every id, and
/// return the entry's new id.
pub(crate) fn prune_unreachable(
    program: &mut Program,
    entry: DeclarationId,
    reachable: &HashSet<DeclarationId>,
) -> DeclarationId {
    // Reachability is closed under the resolved edges before any declaration
    // is dropped: every edge out of a reachable declaration points at another
    // reachable declaration.
    #[cfg(debug_assertions)]
    {
        let violations = crate::compiler::model::verify_reachability(program, entry, reachable);
        debug_assert!(
            violations.is_empty(),
            "the reachable set is not closed before pruning: {violations:?}"
        );
    }
    let mapping = compact(program, reachable);
    let entry = mapped(&mapping, entry);
    remap(program, &mapping);
    // Pruning is the one pass that rewrites every id, so the structural
    // invariant is proved here, after the rewrite. A debug build checks it;
    // a release build skips the walk.
    if cfg!(debug_assertions) {
        crate::compiler::model::verify_program(program);
    }
    #[cfg(debug_assertions)]
    crate::compiler::model::assert_known_relations(
        program,
        crate::compiler::model::RelationBoundary::Pruned,
    );
    entry
}

/// Move the reachable declarations into a fresh Vector, in their old order,
/// and return the old-to-new id mapping; a dropped declaration maps to `None`.
fn compact(
    program: &mut Program,
    reachable: &HashSet<DeclarationId>,
) -> Vec<Option<DeclarationId>> {
    let mut mapping = vec![None; program.declarations.len()];
    let mut declarations = Vec::with_capacity(reachable.len());
    for (index, declaration) in program.declarations.drain(..).enumerate() {
        if reachable.contains(&DeclarationId(index)) {
            mapping[index] = Some(DeclarationId(declarations.len()));
            declarations.push(declaration);
        }
    }
    program.declarations = declarations;
    mapping
}

/// Rewrite every `DeclarationId` occurrence through the mapping, and drop the
/// names and file entries that point at dropped declarations.
fn remap(program: &mut Program, mapping: &[Option<DeclarationId>]) {
    program.module_values.retain(|id| mapping[id.0].is_some());
    for id in &mut program.module_values {
        *id = mapped(mapping, *id);
    }
    for declaration in &mut program.declarations {
        remap_declaration(declaration, mapping);
    }
    for file in &mut program.files {
        file.declarations.retain(|id| mapping[id.0].is_some());
        for id in &mut file.declarations {
            *id = mapped(mapping, *id);
        }
    }
    for namespace in &mut program.namespaces {
        namespace.names.remap(mapping);
        namespace.methods.remap(mapping);
        namespace.associates.remap(mapping);
    }
}

/// Remap the declaration-level edges: the owner, a function's parameters, and
/// the body or initializer the declaration owns. The bodies are rewritten by
/// the edge remapper, which owns every statement and expression variant.
fn remap_declaration(declaration: &mut Declaration, mapping: &[Option<DeclarationId>]) {
    if let Some(owner) = &mut declaration.owner {
        *owner = mapped(mapping, *owner);
    }
    let mut remapper = EdgeRemapper { mapping };
    if let Some(value) = &mut declaration.value {
        walk_expression_mut(&mut remapper, value);
    }
}

/// Rewrites the edges a statement or expression carries: the binding a
/// statement declares, and the declaration a reference or a call names. Every
/// node kind is listed, so a new edge-bearing kind fails to compile.
struct EdgeRemapper<'a> {
    mapping: &'a [Option<DeclarationId>],
}

impl EdgeRemapper<'_> {
    /// Rewrite the declaration a binding statement declares.
    fn declared(&mut self, statement: &mut Statement) {
        let (Statement::Bind { declaration, .. }
        | Statement::Assign { declaration, .. }
        | Statement::FieldAssign { declaration, .. }) = statement
        else {
            unreachable!("the walk names a binding statement");
        };
        *declaration = mapped(self.mapping, *declaration);
    }
}

impl VisitorMut for EdgeRemapper<'_> {
    fn reference(&mut self, expression: &mut Expression) {
        let Expression::Reference { declaration, .. } = expression else {
            unreachable!("the walk names a reference");
        };
        *declaration = mapped(self.mapping, *declaration);
    }

    fn function(&mut self, expression: &mut Expression) {
        let Expression::Function(function) = expression else {
            unreachable!("the walk names a function");
        };
        for parameter in &mut function.parameters {
            *parameter = mapped(self.mapping, *parameter);
        }
        for capture in &mut function.captures {
            *capture = mapped(self.mapping, *capture);
        }
    }

    fn bind(&mut self, statement: &mut Statement) {
        self.declared(statement);
    }
    fn assign(&mut self, statement: &mut Statement) {
        self.declared(statement);
    }
    fn field_assign(&mut self, statement: &mut Statement) {
        self.declared(statement);
    }

    fn variant_pattern(&mut self, pattern: &mut Pattern) {
        let Pattern::Variant { bindings, .. } = pattern else {
            unreachable!("the walk names a variant pattern");
        };
        for id in bindings.iter_mut().flatten() {
            *id = mapped(self.mapping, *id);
        }
    }

    fn text(&mut self, _: &mut Expression) {}
    fn map(&mut self, _: &mut Expression) {}
    fn boolean(&mut self, _: &mut Expression) {}
    fn integer(&mut self, _: &mut Expression) {}
    fn variant(&mut self, _: &mut Expression) {}
    fn interpolated(&mut self, _: &mut Expression) {}
    fn call(&mut self, _: &mut Expression) {}
    fn method(&mut self, _: &mut Expression) {}
    fn field(&mut self, _: &mut Expression) {}
    fn binary(&mut self, _: &mut Expression) {}
    fn unary(&mut self, _: &mut Expression) {}
    fn match_expression(&mut self, _: &mut Expression) {}
    fn expression_statement(&mut self, _: &mut Statement) {}
    fn return_statement(&mut self, _: &mut Statement) {}
    fn panic(&mut self, _: &mut Statement) {}
    fn asynchronous(&mut self, _: &mut Statement) {}
    fn wait(&mut self, _: &mut Statement) {}
    fn forever(&mut self, _: &mut Statement) {}
    fn break_statement(&mut self, _: &mut Statement) {}
    fn value_pattern(&mut self, _: &mut Pattern) {}
    fn expression_body(&mut self, _: &mut MatchBody) {}
    fn block_body(&mut self, _: &mut MatchBody) {}
}

/// The new id of a retained declaration. A dropped id here is a compiler bug:
/// retention records every declaration a retained node points at.
fn mapped(mapping: &[Option<DeclarationId>], id: DeclarationId) -> DeclarationId {
    mapping[id.0].expect("a retained node only points at retained declarations")
}
