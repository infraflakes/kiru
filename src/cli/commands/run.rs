//! `kiru run` command: executes a named run block by resolving its chain
//! of project-function calls through the TUI.

use crate::cli::CliError;
use crate::cli::kiru_toml;
use crate::cli::load_config;
use crate::exec;
use crate::exec::ProjectExec;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

pub(crate) fn execute_run_block(
    config_arg: Option<PathBuf>,
    kirufile_arg: Option<PathBuf>,
    name: String,
) -> Result<(), CliError> {
    let config = load_config(kirufile_arg).map_err(CliError::message)?;

    // A missing kiru.toml is the all-defaults configuration: no projects, so
    // every chain runs at the invocation cwd.
    let toml = kiru_toml::load_kiru_toml_or_default(&crate::cli::get_toml_path(config_arg))
        .map_err(CliError::message)?;
    let mut toml_expanded = toml.clone();
    kiru_toml::expand_project_dirs(&mut toml_expanded);
    let mut projects = BTreeMap::new();
    for (project_name, project) in &toml_expanded.projects {
        if !project.dir.is_empty() {
            projects.insert(
                project_name.clone(),
                ProjectExec {
                    dir: PathBuf::from(&project.dir),
                    direnv: project.direnv,
                },
            );
        }
    }

    let chains = match config.execution_chains.get(&name) {
        Some(stages) => stages.clone(),
        None => {
            return Err(CliError::message(format!("unknown run block '{}'", name)));
        }
    };

    let invocation_cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/"));
    let timeout = toml.timeout.map(std::time::Duration::from_secs);

    exec::chain::execute_task_chains(
        Arc::new(config),
        chains,
        Arc::new(projects),
        invocation_cwd,
        toml.shell.unwrap_or_else(|| "sh".to_string()),
        timeout,
    )
    .map_err(CliError::from)
}
