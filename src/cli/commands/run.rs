//! `kiru run` command: executes a named run block by resolving its chain
//! of project-function calls through the TUI.

use crate::cli::load_config;
use crate::exec;
use std::path::PathBuf;
use std::sync::Arc;

pub(crate) fn execute_run_block(config_arg: Option<PathBuf>, name: String) -> Result<(), String> {
    let config = load_config(config_arg)?;

    let chains = match config.execution_chains.get(&name) {
        Some(stages) => stages.clone(),
        None => {
            return Err(format!("unknown run block '{}'", name));
        }
    };

    exec::chain::execute_task_chains(Arc::new(config), chains)
}
