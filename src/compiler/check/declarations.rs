//! Verification: derive every kind bottom-up and enforce the body rules.

use crate::compiler::{
    DeclarationId, Diagnostic, FileId, Function, Kind, Position, Program, Statement,
};
use crate::syntax::Span;

use super::rules::Walk;

/// Derive every node's kind in declaration order and validate its body. A
/// callee is always declared earlier, so its kind is already settled. Only a
/// file declaration has a body to derive, so a native or a local binding is
/// skipped.
pub(crate) fn analyze(program: &mut Program) -> Result<(), Diagnostic> {
    for index in 0..program.declarations.len() {
        let id = DeclarationId(index);
        let declaration = program.declaration(id);
        let kind = if let Some(function) = declaration.function() {
            verify_function(program, id, program.file_of(id), function)?
        } else if let Some(expression) = declaration.initializer() {
            let mut walk = Walk::new(program, program.file_of(id), false);
            walk.require(expression, Position::TextBinding)?
        } else if let Some(fields) = declaration.fields() {
            let mut walk = Walk::new(program, program.file_of(id), false);
            walk.fields(fields)?;
            Kind::Record
        } else {
            continue;
        };
        program.declaration_mut(id).derived.kind = Some(kind);
    }
    Ok(())
}

/// Enforce the return rules of one function and derive its kind: the kind of
/// its final `return(...)`, or `Nothing` when it has none.
fn verify_function(
    program: &Program,
    id: DeclarationId,
    file: FileId,
    function: &Function,
) -> Result<Kind, Diagnostic> {
    let declaration = program.declaration(id);
    let name = declaration.name.clone();
    if id == program.entry {
        if let Some(span) = disallowed_return(&function.body, false) {
            return Err(Diagnostic::new(
                &program.file(file).path,
                span,
                "`main` must not contain `return`",
            ));
        }
    } else if let Some(span) = disallowed_return(&function.body, true) {
        return Err(Diagnostic::new(
            &program.file(file).path,
            span,
            format!("`{name}` may only `return` as its last statement"),
        ));
    }

    let mut walk = Walk::new(program, file, true);
    walk.statements(&function.body)?;
    Ok(walk.return_kind.unwrap_or(Kind::Nothing))
}

/// The span of a `return` that the rules forbid. When `allow_final` is set,
/// the last statement of this body may be a return; every other return, and
/// every return in a nested `switch`, `case`, `default`, or `defer` body, is
/// disallowed.
fn disallowed_return(body: &[Statement], allow_final: bool) -> Option<Span> {
    for (index, statement) in body.iter().enumerate() {
        match statement {
            Statement::Return { span, .. } => {
                if !(allow_final && index + 1 == body.len()) {
                    return Some(*span);
                }
            }
            Statement::Switch { cases, default, .. } => {
                for case in cases {
                    if let Some(span) = disallowed_return(&case.body, false) {
                        return Some(span);
                    }
                }
                if let Some(default) = default
                    && let Some(span) = disallowed_return(default, false)
                {
                    return Some(span);
                }
            }
            Statement::Defer { body, .. } => {
                if let Some(span) = disallowed_return(body, false) {
                    return Some(span);
                }
            }
            _ => {}
        }
    }
    None
}
