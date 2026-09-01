//! IR builder: consumes the accumulated `LoweringState` and produces
//! the final [`Ir`] with resolved projects and run blocks.

use crate::diagnostics::{Diagnostic, Span};
use crate::ir::{Ir, Project};
use std::collections::BTreeMap;

use super::{CompileError, LoweringState};

pub(super) fn build_ir(state: LoweringState) -> Result<Ir, CompileError> {
    let LoweringState {
        globals: _,
        projects,
        run_blocks,
        source_texts: _,
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

    // Validate run-block references against the projects.
    for (run_name, stages) in &run_blocks {
        for stage in stages {
            for call in stage {
                match ir_projects.get(&call.project) {
                    Some(project) => {
                        if !project.functions.contains_key(&call.function) {
                            return Err(CompileError::Validation(vec![Diagnostic::new(
                                "<run>".to_string(),
                                Span::new(0, 0),
                                format!(
                                    "run `{}`: function `{}` not found in project `{}`",
                                    run_name, call.function, call.project
                                ),
                                String::new(),
                            )]));
                        }
                    }
                    None => {
                        return Err(CompileError::Validation(vec![Diagnostic::new(
                            "<run>".to_string(),
                            Span::new(0, 0),
                            format!("run `{}`: unknown project `{}`", run_name, call.project),
                            String::new(),
                        )]));
                    }
                }
            }
        }
    }

    Ok(Ir {
        projects: ir_projects,
        execution_chains: run_blocks,
    })
}
