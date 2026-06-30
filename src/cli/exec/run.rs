use super::super::load_config;
use crate::runner;
use std::path::PathBuf;
use std::sync::Arc;

/// Executes function chains within the scope of a specific named project.
fn run_project_chains(
    config: Arc<crate::compiler::Sanctuary>,
    project: &str,
    chains: Vec<Vec<String>>,
) -> miette::Result<()> {
    let project = project.to_string();
    runner::chain::execute_task_chains(
        config,
        chains,
        {
            let project = project.clone();
            move |function_name| format!("{}({})", function_name, project)
        },
        move |runner, function_name| runner.execute_fn_call(function_name, &project),
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
            let chains: Vec<Vec<String>> = match project_entry.runs.get(&name) {
                Some(chain_list) => chain_list.clone(),
                None => {
                    return Err(miette::miette!(
                        "unknown run block '{}' in project '{}'",
                        name,
                        project_name
                    ));
                }
            };

            run_project_chains(Arc::new(config), project_name, chains)
        }
        None => Err(miette::miette!(
            "must specify a project to run '{}' in",
            name
        )),
    }
}
