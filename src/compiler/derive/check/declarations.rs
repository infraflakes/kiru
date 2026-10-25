//! Verification: prove every function body against its declared type, then
//! settle and report the flow rules.
//!
//! Typing is one job and needs no flow fact: a body is typed once, in
//! declaration order, against the types settled by the linker. Whether a body
//! falls through, breaks out of a loop, or returns where it may not is a
//! statement fact, so the flow pass owns it and reads the immutable model
//! afterwards. The checker reads the immutable graph and records what it
//! derives in `Facts`; it never writes a node type or takes a body out. Every
//! body is attempted: one body's error never hides another file's fact.

use crate::compiler::Diagnostic;
use crate::compiler::graph::{DeclarationId, Facts, ModelView, Program};
use crate::compiler::{Report, ReportRule};

use super::flow_rules::check_flow;
use super::statement_check::Checker;

/// Validate the program: every function body and module value initializer.
/// The returned facts are the settled store beside the unchanged graph.
pub(crate) fn validate_program(program: &Program) -> (Facts, Report) {
    let mut facts = Facts::default();
    let mut report = Report::new();
    report.extend(ReportRule::Check, check_bodies(program, &mut facts));
    report.extend(ReportRule::Check, check_flow(program, &mut facts));
    #[cfg(debug_assertions)]
    if report.is_empty() {
        crate::compiler::assert_relations(
            program,
            &facts,
            crate::compiler::RelationBoundary::Checked,
        );
    }
    (facts, report)
}

/// Type every body once, keeping the first error each body earns. The declared
/// type is read from the declaration, and the body is read where it stands, so
/// the checker holds only shared references to the graph and writes only the
/// facts store.
fn check_bodies(program: &Program, facts: &mut Facts) -> Vec<Diagnostic> {
    let mut diagnostics = Vec::new();
    for index in 0..program.declarations.len() {
        let id = DeclarationId(index);
        let declaration = program.declaration(id);
        let kind = declaration.kind;
        let has_function_body = kind.callable() && kind.has_body();
        if !has_function_body && !kind.module_value() {
            continue;
        }
        let return_type = if has_function_body {
            declaration.return_type(&program.declarations)
        } else {
            None
        };
        let declared = kind.module_value().then(|| declaration.ty.clone());
        let file = program.file_of(id);
        let namespace = declaration.namespace;
        let mut walk = Checker::new(ModelView::from(program), facts, id, file, namespace);
        let outcome = if has_function_body {
            let function = declaration
                .function()
                .expect("a callable body is a function expression");
            walk.check_function(function, return_type)
        } else {
            let value = declaration
                .value()
                .expect("a module value carries its initializer");
            walk.require(
                value,
                declared
                    .as_ref()
                    .expect("a module value carries its declared type"),
            )
            .map(|_| ())
        };
        if let Err(diagnostic) = outcome {
            diagnostics.push(diagnostic);
        }
    }
    diagnostics
}
