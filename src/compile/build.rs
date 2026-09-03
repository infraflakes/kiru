//! IR builder: consumes the accumulated `CompileState` and produces
//! the final [`Ir`] with resolved projects and run blocks.

use crate::diagnostics::{Diagnostic, Span};
use crate::ir::{Ir, Project};
use std::collections::BTreeMap;

use super::{CompileError, CompileState, PendingRunBlock};

pub(super) fn build_ir(state: CompileState) -> Result<Ir, CompileError> {
    let CompileState {
        globals: _,
        projects,
        run_blocks,
        source_texts,
        loaded_files: _,
        recursion_stack: _,
    } = state;

    let mut ir_projects = BTreeMap::new();
    for (name, pending) in projects {
        ir_projects.insert(
            name,
            Project {
                functions: pending.functions,
            },
        );
    }

    // Validate run-block references against the projects. Validation lives
    // here (after every file is compiled) because a run block may reference
    // projects declared later; the stored declaration span keeps the
    // diagnostic anchored to the real source location.
    let mut ir_chains = BTreeMap::new();
    for (run_name, pending_run) in run_blocks {
        let PendingRunBlock {
            stages,
            source_name,
            offset,
            len,
        } = pending_run;
        for stage in &stages {
            for call in stage {
                let error_message = match ir_projects.get(&call.project) {
                    Some(project) => {
                        if project.functions.contains_key(&call.function) {
                            continue;
                        }
                        Some(format!(
                            "run `{}`: function `{}` not found in project `{}`",
                            run_name, call.function, call.project
                        ))
                    }
                    None => Some(format!(
                        "run `{}`: unknown project `{}`",
                        run_name, call.project
                    )),
                };
                if let Some(message) = error_message {
                    return Err(CompileError::diagnostic(Diagnostic::new(
                        source_name.clone(),
                        Span::new(offset, len.max(1)),
                        message,
                        source_texts.get(&source_name).cloned().unwrap_or_default(),
                    )));
                }
            }
        }
        ir_chains.insert(run_name, stages);
    }

    Ok(Ir {
        projects: ir_projects,
        execution_chains: ir_chains,
    })
}
