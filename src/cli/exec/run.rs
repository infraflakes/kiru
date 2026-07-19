use super::super::load_config;
use crate::dsl::ast::QualifiedFnRef;
use crate::plan::Plan;
use crate::runner;
use std::path::PathBuf;
use std::sync::Arc;

/// Executes function chains within the scope of a specific named project.
/// A qualified reference (`nix::build`) targets the named project, re-using
/// its configured working directory without altering the current project.
fn run_project_chains(config: Arc<Plan>, chains: Vec<Vec<QualifiedFnRef>>) -> miette::Result<()> {
    runner::chain::execute_task_chains(
        config,
        chains,
        move |q: &QualifiedFnRef| format!("{}({})", q.function, q.project),
        move |runner, q: &QualifiedFnRef| runner.execute_fn_call(&q.function, &q.project),
    )
}

/// Entry point for the run CLI command, dispatches to the correct runner
/// based on whether a project was specified.
pub fn execute_run_block(
    config_arg: Option<PathBuf>,
    name: String,
    project: Option<String>,
) -> miette::Result<()> {
    let config = load_config(config_arg)?;

    match project {
        Some(ref project_name) => {
            if !config.projects.contains_key(project_name) {
                return Err(miette::miette!("unknown project: {}", project_name));
            }

            let project_entry = &config.projects[project_name];
            let chains: Vec<Vec<QualifiedFnRef>> = match project_entry.runs.get(&name) {
                Some(chain_list) => chain_list.clone(),
                None => {
                    return Err(miette::miette!(
                        "unknown run block '{}' in project '{}'",
                        name,
                        project_name
                    ));
                }
            };

            run_project_chains(Arc::new(config), chains)
        }
        None => Err(miette::miette!(
            "must specify a project to run '{}' in",
            name
        )),
    }
}
