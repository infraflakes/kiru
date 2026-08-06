use super::super::load_config;
use crate::dsl::ast::QualifiedFnRef;
use crate::runner;
use std::path::PathBuf;
use std::sync::Arc;

/// Entry point for the run CLI command, executes a global run block by name.
/// Each chain reference is `namespace::function`; the namespace is the project name.
pub fn execute_run_block(config_arg: Option<PathBuf>, name: String) -> miette::Result<()> {
    let config = load_config(config_arg)?;

    let chains: Vec<Vec<QualifiedFnRef>> = match config.runs.get(&name) {
        Some(chain_list) => chain_list.clone(),
        None => {
            return Err(miette::miette!("unknown run block '{}'", name));
        }
    };

    runner::chain::execute_task_chains(Arc::new(config), chains)
}
