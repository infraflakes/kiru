use super::super::load_config;
use crate::runner;
use std::path::PathBuf;
use std::sync::Arc;

/// Entry point for the run CLI command, executes a global run block by name.
/// Each chain reference is `project::function`; the project is the namespace.
pub fn execute_run_block(config_arg: Option<PathBuf>, name: String) -> miette::Result<()> {
    let config = load_config(config_arg)?;

    let chains = match config.run_blocks.get(&name) {
        Some(stages) => stages.clone(),
        None => {
            return Err(miette::miette!("unknown run block '{}'", name));
        }
    };

    runner::chain::execute_task_chains(Arc::new(config), chains)
}
