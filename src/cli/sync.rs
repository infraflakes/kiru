use crate::compiler::CompileError;
use crate::runner;
use std::path::PathBuf;

/// Sync all projects via the TUI.
pub fn run_sync_command(config_arg: Option<PathBuf>) -> miette::Result<()> {
    let config_path = super::get_config_path(config_arg);
    let config = crate::compiler::extract_projects(&config_path).map_err(|e| match e {
        CompileError::ParseReports(reports) => super::print_parse_errors(reports),
        _ => miette::miette!("{}", e),
    })?;

    let chain_pairs: Vec<(String, Vec<String>)> = config
        .projects
        .keys()
        .map(|name| (name.clone(), vec![name.clone()]))
        .collect();
    runner::sync::run_sync_for_projects(config.projects, chain_pairs)
}
