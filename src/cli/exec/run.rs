use super::super::load_config;
use crate::dsl::ast::QualifiedFnRef;
use crate::plan::Plan;
use crate::runner;
use std::path::PathBuf;
use std::sync::Arc;

/// Executes function chains from the global run blocks.
/// Each chain reference is `namespace::function`; the namespace is the project name.
fn run_project_chains(config: Arc<Plan>, chains: Vec<Vec<QualifiedFnRef>>) -> miette::Result<()> {
    runner::chain::execute_task_chains(
        config,
        chains,
        move |q: &QualifiedFnRef| format!("{}::{}", q.project, q.function),
        move |runner, q: &QualifiedFnRef| runner.execute_fn_call(&q.function, &q.project),
    )
}

/// Entry point for the run CLI command, executes a global run block by name.
pub fn execute_run_block(config_arg: Option<PathBuf>, name: String) -> miette::Result<()> {
    let config = load_config(config_arg)?;

    let chains: Vec<Vec<QualifiedFnRef>> = match config.runs.get(&name) {
        Some(chain_list) => chain_list.clone(),
        None => {
            return Err(miette::miette!("unknown run block '{}'", name));
        }
    };

    run_project_chains(Arc::new(config), chains)
}
