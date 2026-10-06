//! Compaction and remapping: rewrite the model to the reachable set.

use std::collections::HashSet;

use crate::model::{
    Declaration, DeclarationId, Expression, Program, Statement, VisitorMut, walk_expression_mut,
    walk_statements_mut,
};

use super::mark_reachable::collect_reachable_declarations;

/// Keep only the declarations the entry reaches, remapping every id.
pub(crate) fn prune_unreachable(program: &mut Program) {
    let reachable = collect_reachable_declarations(program);
    let mapping = compact(program, &reachable);
    remap(program, &mapping);
    crate::model::verify_program(program);
}

/// Move the reachable declarations into a fresh vector, in their old order,
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
    program.entry = mapped(mapping, program.entry);
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
        namespace.values.remap(mapping);
        namespace.functions.remap(mapping);
    }
}

/// Remap the declaration-level edges: the owner, a function's parameters, and
/// the body or initializer the declaration owns. The bodies are rewritten by
/// the edge remapper, which owns every statement and expression variant.
fn remap_declaration(declaration: &mut Declaration, mapping: &[Option<DeclarationId>]) {
    if let Some(owner) = &mut declaration.owner {
        *owner = mapped(mapping, *owner);
    }
    for parameter in &mut declaration.parameters {
        *parameter = mapped(mapping, *parameter);
    }
    let mut remapper = EdgeRemapper { mapping };
    if let Some(function) = declaration.function_mut() {
        walk_statements_mut(&mut remapper, &mut function.body);
    } else if let Some(expression) = declaration.initializer_mut() {
        walk_expression_mut(&mut remapper, expression);
    }
}

/// Rewrites the edges a statement or expression carries: the binding a
/// statement declares, and the declaration a reference or a call names. Every
/// node kind is listed, so a new edge-bearing kind fails to compile.
struct EdgeRemapper<'a> {
    mapping: &'a [Option<DeclarationId>],
}

impl VisitorMut for EdgeRemapper<'_> {
    fn expression(&mut self, expression: &mut Expression) {
        match expression {
            Expression::Reference { declaration, .. } => {
                *declaration = mapped(self.mapping, *declaration);
            }
            Expression::Call { callee, .. } => {
                *callee = mapped(self.mapping, *callee);
            }
            Expression::Text { .. }
            | Expression::Record { .. }
            | Expression::List { .. }
            | Expression::Field { .. }
            | Expression::Add { .. } => {}
        }
    }

    fn statement(&mut self, statement: &mut Statement) {
        match statement {
            Statement::Bind { declaration, .. }
            | Statement::Assign { declaration, .. }
            | Statement::FieldAssign { declaration, .. } => {
                *declaration = mapped(self.mapping, *declaration);
            }
            Statement::ForEach { item, .. } => {
                *item = mapped(self.mapping, *item);
            }
            Statement::Expression(_)
            | Statement::Return { .. }
            | Statement::Panic { .. }
            | Statement::Async { .. }
            | Statement::Wait { .. }
            | Statement::Switch { .. }
            | Statement::Break { .. }
            | Statement::Forever { .. } => {}
        }
    }
}

/// The new id of a retained declaration. A dropped id here is a compiler bug:
/// retention records every declaration a retained node points at.
fn mapped(mapping: &[Option<DeclarationId>], id: DeclarationId) -> DeclarationId {
    mapping[id.0].expect("a retained node only points at retained declarations")
}
