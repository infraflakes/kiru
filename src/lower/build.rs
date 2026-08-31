use crate::diagnostics::{Diagnostic, Span};
use crate::ir::{Ir, Project, Sync};
use crate::syntax::Template;
use std::collections::BTreeMap;

use super::inline::lower_template;
use super::{CompileError, LoweringState};

pub(super) fn build_ir(state: LoweringState) -> Result<Ir, CompileError> {
    let shell = state.shell.clone().unwrap_or_else(|| "sh".to_string());
    let timeout = state.timeout.ok_or_else(|| {
        CompileError::Validation(vec![Diagnostic::new(
            "<config>".to_string(),
            Span::new(0, 0),
            "missing mandatory `timeout = (<seconds>);` declaration",
            String::new(),
        )])
    })?;

    let mut repositories = BTreeMap::new();
    for (name, s) in &state.syncs {
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

    let mut projects = BTreeMap::new();
    // Every name that appears in either map gets a project entry.
    let mut names: Vec<String> = state.projects.keys().cloned().collect();
    for name in state.syncs.keys() {
        if !names.contains(name) {
            names.push(name.clone());
        }
    }
    for name in names {
        let functions = match state.projects.get(&name) {
            Some(pending) => pending.functions.clone(),
            None => BTreeMap::new(),
        };
        projects.insert(name, Project { functions });
    }

    // Validate run-block references against the merged projects.
    for (run_name, stages) in &state.run_blocks {
        for stage in stages {
            for call in stage {
                match projects.get(&call.project) {
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
        projects,
        execution_chains: state.run_blocks,
    })
}
