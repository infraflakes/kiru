//! Verification: derive every kind bottom-up and enforce the body rules.

use crate::compiler::{
    DeclarationId, DeclarationKind, Diagnostic, FileId, Function, Kind, Position, Program,
    Statement,
};

use super::validate_bodies::Walk;

/// Derive every node's kind in declaration order and validate its body. A
/// callee is always declared earlier, so its kind is already settled. Only a
/// file declaration has a body to derive, so a native or a local binding is
/// skipped.
pub(crate) fn validate_program(program: &mut Program) -> Result<(), Diagnostic> {
    for index in 0..program.declarations.len() {
        let id = DeclarationId(index);
        let declaration = program.declaration(id);
        let kind = if let Some(function) = declaration.function() {
            verify_function(program, id, program.file_of(id), function)?
        } else if let Some(expression) = declaration.initializer() {
            let position = match &declaration.kind {
                DeclarationKind::Record(_) => Position::RecordBinding,
                _ => Position::TextBinding,
            };
            let mut walk = Walk::new(program, program.file_of(id));
            walk.require(expression, position)?
        } else {
            continue;
        };
        program.declaration_mut(id).derived.kind = Some(kind);
    }
    Ok(())
}

/// Enforce the flow rule of one function and derive its kind.
///
/// A function ends when its body ends. `return` is an early exit: `return()`
/// carries nothing, `return(expr)` carries text or record. Every return in one
/// function is the same kind, and a function whose kind is not `nothing` must
/// not fall through, so the call always produces the kind it is marked with.
/// `main` is an ordinary function; the runtime discards its value.
fn verify_function(
    program: &Program,
    id: DeclarationId,
    file: FileId,
    function: &Function,
) -> Result<Kind, Diagnostic> {
    let declaration = program.declaration(id);
    let name = declaration.name.clone();
    let path = &program.file(file).path;

    let mut walk = Walk::new(program, file);
    walk.statements(&function.body)?;

    let kind = walk.return_kind.unwrap_or(Kind::Nothing);
    if kind != Kind::Nothing && !terminates(&function.body) {
        return Err(Diagnostic::new(
            path,
            declaration.name_span,
            format!(
                "`{name}` returns {} but can fall through; every path must end in `return` or `panic`",
                kind.name()
            ),
        ));
    }
    Ok(kind)
}

/// Whether a body's flow always ends in a terminator. A block terminates when
/// any statement terminates, because the statements after it are unreachable;
/// a `switch` terminates when it has a `default` and every arm terminates;
/// `defer` schedules cleanup and never terminates.
fn terminates(body: &[Statement]) -> bool {
    body.iter().any(statement_terminates)
}

fn statement_terminates(statement: &Statement) -> bool {
    match statement {
        Statement::Return { .. } | Statement::Panic { .. } => true,
        Statement::Switch {
            cases,
            default: Some(default),
            ..
        } => cases.iter().all(|case| terminates(&case.body)) && terminates(default),
        _ => false,
    }
}
