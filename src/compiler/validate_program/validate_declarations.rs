//! Verification: prove every function body against its declared type, and
//! learn which functions stop the run.

use crate::compiler::Diagnostic;
use crate::model::{DeclarationId, DeclarationKind, FileId, Function, Program};
use crate::types::Type;

use super::validate_bodies::Walk;

/// Validate every declaration in declaration order. A callee is always
/// declared earlier, so its type and whether it stops the run are already
/// settled. A function's type comes from its declaration, and a module value's
/// type is written in its own declaration, so only a function body and a module
/// value initializer are proved.
pub(crate) fn validate_program(program: &mut Program) -> Result<(), Diagnostic> {
    for index in 0..program.declarations.len() {
        let id = DeclarationId(index);
        let declaration = program.declaration(id);
        if let Some(function) = declaration.function() {
            let return_type = function.return_type;
            let halts = verify_function(program, id, program.file_of(id), function, return_type)?;
            program.declaration_mut(id).derived.halts = halts;
        } else if let Some(expression) = declaration.initializer() {
            let ty = match &declaration.kind {
                DeclarationKind::Value { ty, .. } => *ty,
                other => {
                    unreachable!("only a module value has an initializer, found {other:?}")
                }
            };
            let mut walk = Walk::new(program, program.file_of(id));
            walk.require(expression, ty)?;
        }
    }
    Ok(())
}

/// Prove one function body against its declared return type and report whether
/// every path stops the run.
///
/// A function declared `-> txt` or `-> rec` must not fall through: every path
/// ends in `return expr;`, `panic;`, or a call to a function that stops the
/// run. A function declared with no return type may fall through. `main` is an
/// ordinary function; the runtime discards its value.
fn verify_function(
    program: &Program,
    id: DeclarationId,
    file: FileId,
    function: &Function,
    return_type: Type,
) -> Result<bool, Diagnostic> {
    let declaration = program.declaration(id);
    let name = &declaration.name;
    let path = &program.file(file).path;

    let mut walk = Walk::for_function(program, file, name, return_type);
    let flow = walk.statements(&function.body)?;

    if return_type != Type::Void && flow.falls {
        return Err(Diagnostic::new(
            path,
            function
                .body
                .last()
                .map_or(declaration.name_span, |statement| statement.span()),
            format!(
                "`{name}` is declared to return {}, so every path must end with `return ...`, `panic;`, or a call that stops the run",
                return_type.name()
            ),
        ));
    }
    Ok(!flow.falls && !flow.returns)
}
