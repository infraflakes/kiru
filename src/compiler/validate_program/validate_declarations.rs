//! Verification: prove every function body against its declared type, and
//! learn which functions stop the run.

use crate::compiler::Diagnostic;
use crate::compiler::model::Type;
use crate::compiler::model::{DeclarationId, FileId, Function, Program};

use super::statement_check::Checker;

/// Validate every declaration in declaration order. A callee is always
/// declared earlier, so its type and whether it stops the run are already
/// settled. A function's type comes from its declaration, and a module value's
/// type is written in its own declaration, so only a function body and a module
/// value initializer are proved.
pub(crate) fn validate_program(program: &mut Program) -> Result<(), Diagnostic> {
    for index in 0..program.declarations.len() {
        let id = DeclarationId(index);
        let declaration = program.declaration(id);
        if declaration.kind.callable() && declaration.kind.has_body() {
            let function = declaration
                .function()
                .expect("a callable body is a function expression");
            // A function with no return type has no return type to check
            // against; its calls produce no value.
            let return_type = declaration.return_type(&program.types);
            let halts = verify_function(program, id, program.file_of(id), function, return_type)?;
            program.declaration_mut(id).derived.halts = halts;
        } else if declaration.kind.has_value() && declaration.owner.is_none() {
            let ty = declaration.ty.clone();
            let namespace = declaration.namespace;
            let value = declaration
                .value()
                .expect("a module value carries its initializer");
            let mut walk = Checker::new(program, program.file_of(id), namespace);
            walk.require(value, &ty)?;
        }
    }
    Ok(())
}

/// Prove one function body against its declared return type and report whether
/// every path stops the run.
///
/// A function declared with a return type must not fall through: every path
/// ends in `return expr;`, `panic;`, or a call to a function that stops the
/// run. A function declared with no return type may fall through. `main` is an
/// ordinary function; the runtime discards its value.
fn verify_function(
    program: &Program,
    id: DeclarationId,
    file: FileId,
    function: &Function,
    return_type: Option<Type>,
) -> Result<bool, Diagnostic> {
    let name = program.declaration(id).name.as_str();
    let namespace = program.declaration(id).namespace;
    let mut walk = Checker::for_function(program, file, namespace, name, return_type);
    let flow = walk.check_function(function, name)?;
    Ok(!flow.falls && !flow.returns)
}
