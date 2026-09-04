//! `kiru run` command: executes a named run block by resolving its chain
//! of project-function calls through the TUI.

use crate::cli::CliError;
use crate::cli::kiru_toml;
use crate::cli::load_config;
use crate::exec;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

pub(crate) fn execute_run_block(
    config_arg: Option<PathBuf>,
    kirufile_arg: Option<PathBuf>,
    name: String,
) -> Result<(), CliError> {
    let config = load_config(kirufile_arg).map_err(CliError::message)?;

    let toml = kiru_toml::load_kiru_toml_at(&crate::cli::get_toml_path(config_arg))
        .map_err(CliError::message)?;
    let mut repo_dirs = BTreeMap::new();
    let mut toml_expanded = toml.clone();
    kiru_toml::expand_repo_dirs(&mut toml_expanded);
    for repo in &toml_expanded.repos {
        if !repo.dir.is_empty() {
            repo_dirs.insert(repo.name.clone(), PathBuf::from(&repo.dir));
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
        toml.shell,
        timeout,
        repo_dirs,
        invocation_cwd,
        toml.direnv,
    )
    .map_err(CliError::from)
}
