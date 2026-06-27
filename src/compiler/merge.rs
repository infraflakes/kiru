use crate::compiler::error::{CompileError, spanned_err};
use crate::compiler::types::UnresolvedProject;
use crate::dsl::{ProjectField, Stmt};
use std::collections::HashSet;

/// Merge a single statement into a project body during AST collection.
///
/// No expression resolution or shell execution occurs — all values are stored
/// as raw `Expr` nodes and var declarations are stored as raw `Stmt::Var` nodes.
pub(crate) fn merge_project_body_stmt(
    project: &mut UnresolvedProject,
    stmt: Stmt,
    source_name: &str,
    source_text: &str,
    seen_fields: &mut HashSet<String>,
) -> Result<(), CompileError> {
    let make_err = |msg: String, offset: usize, len: usize| -> CompileError {
        spanned_err(msg, source_name, source_text, offset, len)
    };
    match stmt {
        // Var stmts were already resolved during linear processing and
        // do not need to be stored in the project struct.
        Stmt::Var { .. } => {}
        Stmt::Field {
            key,
            value,
            offset,
            len,
            ..
        } => {
            let field_name = format!("{:?}", key);
            if !seen_fields.insert(field_name) {
                return Err(make_err(
                    format!("duplicate field '{:?}' in project '{}'", key, project.name),
                    offset,
                    len,
                ));
            }

            match key {
                ProjectField::Url => project.url = Some(value),
                ProjectField::Dir => project.dir = Some(value),
                ProjectField::Sync => project.sync = Some(value),
                ProjectField::Branch => project.branch = Some(value),
            }
        }
        Stmt::Fn {
            name,
            body,
            offset,
            len,
            ..
        } => {
            if project.functions.contains_key(&name) {
                return Err(make_err(
                    format!("duplicate function in project '{}': {}", project.name, name),
                    offset,
                    len,
                ));
            }
            project.functions.insert(name, body);
        }
        Stmt::Run {
            name,
            chains,
            offset,
            len,
            ..
        } => {
            if project.runs.contains_key(&name) {
                return Err(make_err(
                    format!(
                        "duplicate run block in project '{}': {}",
                        project.name, name
                    ),
                    offset,
                    len,
                ));
            }
            project.runs.insert(name, chains);
        }
        Stmt::Sanctuary { .. } | Stmt::Project { .. } => {
            return Err(spanned_err(
                format!(
                    "unexpected statement in project '{}' (only var, fn, run, and fields are valid)",
                    project.name
                ),
                source_name,
                source_text,
                0,
                1,
            ));
        }
    }
    Ok(())
}
