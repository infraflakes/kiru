//! IR builder: consumes the accumulated `LoweringState` and produces
//! the final [`Ir`] with resolved repositories, projects, and run blocks.

use crate::diagnostics::{Diagnostic, Span};
use crate::ir::{Ir, Project, Sync};
use crate::syntax::Template;
use std::collections::BTreeMap;

use super::inline::lower_template;
use super::{CompileError, LoweringState};

pub(super) fn build_ir(state: LoweringState) -> Result<Ir, CompileError> {
    let shell = state.shell.unwrap_or_else(|| "sh".to_string());
    let timeout = state.timeout.ok_or_else(|| {
        CompileError::Validation(vec![Diagnostic::new(
            "<config>".to_string(),
            Span::new(0, 0),
            "missing mandatory `timeout = (<seconds>);` declaration",
            String::new(),
        )])
    })?;

    let LoweringState {
        shell: _,
        timeout: _,
        globals: _,
        syncs,
        projects,
        run_blocks,
        source_texts: _,
        loaded_files: _,
        recursion_stack: _,
    } = state;

    let mut repositories = BTreeMap::new();
    for (name, s) in &syncs {
        repositories.insert(
            name.clone(),
            Sync {
                url: lower_template(s.url.as_ref().unwrap_or(&Template::default())),
                dir: lower_template(s.dir.as_ref().unwrap_or(&Template::default())),
                branch: lower_template(s.branch.as_ref().unwrap_or(&Template::default())),
                strategy: lower_template(s.strategy.as_ref().unwrap_or(&Template::lit("clone"))),
            },
        );
    }

    let mut ir_projects = BTreeMap::new();
    // Every name that appears in either map gets a project entry.
    let mut names: Vec<String> = projects.keys().cloned().collect();
    for name in syncs.keys() {
        if !names.contains(name) {
            names.push(name.clone());
        }
    }
    let mut projects_map = projects;
    for name in names {
        let functions = match projects_map.remove(&name) {
            Some(pending) => pending.functions,
            None => BTreeMap::new(),
        };
        ir_projects.insert(name, Project { functions });
    }

    // Validate run-block references against the merged projects.
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
        shell,
        timeout,
        repositories,
        projects: ir_projects,
        execution_chains: run_blocks,
    })
}
