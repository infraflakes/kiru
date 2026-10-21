//! Verification: prove every function body against its declared type, then
//! settle and report the flow rules.
//!
//! Typing is one job and needs no flow fact: a body is typed once, in
//! declaration order, against the types settled by the linker. Whether a body
//! falls through, breaks out of a loop, or returns where it may not is a
//! statement fact, so the flow pass owns it and reads the typed model
//! afterwards.

use crate::compiler::Diagnostic;
use crate::compiler::model::{DeclarationId, Expression, FileId, Function, ModelView, Program};

use super::flow_check::check_flow;
use super::statement_check::Checker;

/// Validate the program: every function body and module value initializer.
pub(crate) fn validate_program(program: &mut Program) -> Result<(), Diagnostic> {
    validate_bodies(program, None)
}

/// Validate only the bodies declared in `file`.
pub(crate) fn validate_file(program: &mut Program, file: FileId) -> Result<(), Diagnostic> {
    validate_bodies(program, Some(file))
}

/// Run the one rule set over the bodies the filter admits: typing first, then
/// flow, so no rule is declared twice.
fn validate_bodies(program: &mut Program, only_file: Option<FileId>) -> Result<(), Diagnostic> {
    check_bodies(program, only_file)?;
    check_flow(program, only_file)
}

/// Type every admitted body once. The declared type is read from the
/// declaration, and the body is taken out while the checker holds the rest of
/// the program, so the one write it records (a variant-pattern binding's type)
/// lands on the node that owns it.
fn check_bodies(program: &mut Program, only_file: Option<FileId>) -> Result<(), Diagnostic> {
    for index in 0..program.declarations.len() {
        let id = DeclarationId(index);
        if !in_file(program, id, only_file) {
            continue;
        }
        let kind = program.declaration(id).kind;
        let has_function_body = kind.callable() && kind.has_body();
        if !has_function_body && !kind.module_value() {
            continue;
        }
        let return_type = if has_function_body {
            program.declaration(id).return_type(&program.types)
        } else {
            None
        };
        let declared = kind
            .module_value()
            .then(|| program.declaration(id).ty.clone());
        let file = program.file_of(id);
        let namespace = program.declaration(id).namespace;
        let mut value = program
            .declaration_mut(id)
            .value
            .take()
            .expect("a body declaration carries its value");
        let outcome = {
            if has_function_body {
                let Expression::Function(function) = &mut value else {
                    unreachable!("a callable body is a function expression");
                };
                // While the body is checked, the declaration keeps the
                // function's metadata so a reference to the function itself
                // reads its parameters, captures and declared type. Only the
                // body, which the walk owns, is absent.
                program.declaration_mut(id).value = Some(Expression::Function(Function {
                    declared_return: function.declared_return.clone(),
                    parameters: function.parameters.clone(),
                    body: Vec::new(),
                    type_parameters: function.type_parameters.clone(),
                    captures: function.captures.clone(),
                    span: function.span,
                }));
                let mut walk = Checker::new(ModelView::from(&mut *program), file, namespace);
                walk.check_function(function, return_type)
            } else {
                let mut walk = Checker::new(ModelView::from(&mut *program), file, namespace);
                walk.require(
                    &mut value,
                    declared
                        .as_ref()
                        .expect("a module value carries its declared type"),
                )
                .map(|_| ())
            }
        };
        program.declaration_mut(id).value = Some(value);
        outcome?;
    }
    Ok(())
}

/// Whether the declaration's body is in the checked set: all bodies, or only
/// the ones the file declares.
fn in_file(program: &Program, id: DeclarationId, only_file: Option<FileId>) -> bool {
    match only_file {
        Some(file) => program.declaration(id).file == Some(file),
        None => true,
    }
}
