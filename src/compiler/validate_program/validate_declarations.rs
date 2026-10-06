//! Verification: prove every function body against its declared kind, derive
//! the remaining kinds bottom-up, and learn which functions stop the run.

use crate::compiler::Diagnostic;
use crate::model::{DeclarationId, DeclarationKind, FileId, Function, Kind, Position, Program};

use super::validate_bodies::Walk;

/// Validate every declaration in declaration order. A callee is always
/// declared earlier, so its kind and whether it stops the run are already
/// settled. A function's kind comes from its declaration; only a module value
/// has a kind to derive, so a native or a local binding is skipped.
pub(crate) fn validate_program(program: &mut Program) -> Result<(), Diagnostic> {
    for index in 0..program.declarations.len() {
        let id = DeclarationId(index);
        let declaration = program.declaration(id);
        if let Some(function) = declaration.function() {
            let declared_kind = function.return_kind;
            let halts = verify_function(program, id, program.file_of(id), function, declared_kind)?;
            program.declaration_mut(id).derived.halts = halts;
        } else if let Some(expression) = declaration.initializer() {
            let position = match &declaration.kind {
                DeclarationKind::Text(_) => Position::TextBinding,
                DeclarationKind::Record(_) => Position::RecordBinding,
                DeclarationKind::List(_) => Position::ListBinding,
                other => {
                    unreachable!(
                        "only a text, record, or list value has an initializer, found {other:?}"
                    )
                }
            };
            let mut walk = Walk::new(program, program.file_of(id));
            let kind = walk.require(expression, position)?;
            program.declaration_mut(id).derived.kind = Some(kind);
        }
    }
    Ok(())
}

/// Prove one function body against its declared kind and report whether every
/// path stops the run.
///
/// A function declared `-> txt` or `-> rec` must not fall through: every path
/// ends in `return(expr);`, `panic;`, or a call to a function that stops the
/// run. A function declared with no return kind may fall through. `main` is an
/// ordinary function; the runtime discards its value.
fn verify_function(
    program: &Program,
    id: DeclarationId,
    file: FileId,
    function: &Function,
    declared_kind: Kind,
) -> Result<bool, Diagnostic> {
    let declaration = program.declaration(id);
    let name = &declaration.name;
    let path = &program.file(file).path;

    let mut walk = Walk::for_function(program, file, name, declared_kind);
    let flow = walk.statements(&function.body)?;

    if declared_kind != Kind::Nothing && flow.falls {
        return Err(Diagnostic::new(
            path,
            function
                .body
                .last()
                .map_or(declaration.name_span, |statement| statement.span()),
            format!(
                "`{name}` is declared to return {}, so every path must end with `return(...)`, `panic;`, or a call that stops the run",
                declared_kind.name()
            ),
        ));
    }
    Ok(!flow.falls && !flow.returns)
}
