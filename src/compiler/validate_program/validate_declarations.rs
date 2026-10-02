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

/// Enforce the flow rules of one function and derive its kind.
///
/// A function ends when its body ends. `return` is an early exit: a value
/// return carries text or record, a valueless return ends a void function.
/// Every value return in one function is the same kind, and a value function
/// must not fall through, so the call always produces the kind it is marked
/// with. `main` is void for every caller because the runtime discards its
/// value, so a return there is allowed and does not set its kind.
fn verify_function(
    program: &Program,
    id: DeclarationId,
    file: FileId,
    function: &Function,
) -> Result<Kind, Diagnostic> {
    let declaration = program.declaration(id);
    let name = declaration.name.clone();
    let path = &program.file(file).path;

    if let Some(span) = return_in_defer(&function.body) {
        return Err(Diagnostic::new(
            path,
            span,
            format!("`{name}` must not `return` inside `defer`"),
        ));
    }

    let mut walk = Walk::new(program, file, true);
    walk.statements(&function.body)?;

    if id == program.entry {
        return Ok(Kind::Nothing);
    }

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
/// a `switch` terminates when it has a `default` and every arm terminates.
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

/// The span of any `return` inside a `defer` body, including a nested one.
fn return_in_defer(body: &[Statement]) -> Option<Span> {
    for statement in body {
        match statement {
            Statement::Defer { body, .. } => {
                if let Some(span) = find_return(body) {
                    return Some(span);
                }
            }
            Statement::Switch { cases, default, .. } => {
                for case in cases {
                    if let Some(span) = return_in_defer(&case.body) {
                        return Some(span);
                    }
                }
                if let Some(default) = default
                    && let Some(span) = return_in_defer(default)
                {
                    return Some(span);
                }
            }
            _ => {}
        }
    }
    None
}

/// The span of any `return` in a body, at any depth.
fn find_return(body: &[Statement]) -> Option<Span> {
    for statement in body {
        match statement {
            Statement::Return { span, .. } => return Some(*span),
            Statement::Switch { cases, default, .. } => {
                for case in cases {
                    if let Some(span) = find_return(&case.body) {
                        return Some(span);
                    }
                }
                if let Some(default) = default
                    && let Some(span) = find_return(default)
                {
                    return Some(span);
                }
            }
            Statement::Defer { body, .. } => {
                if let Some(span) = find_return(body) {
                    return Some(span);
                }
            }
            _ => {}
        }
    }
    None
}
