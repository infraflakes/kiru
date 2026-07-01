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

    let projects: Vec<(String, crate::compiler::Project)> = config
        .projects
        .into_iter()
        .filter(|(name, proj)| {
            if proj.url.is_empty() && proj.dir.is_empty() {
                eprintln!("{:?}", miette::miette!("project {:?}: missing url and dir, skipping sync", name));
                false
            } else if proj.url.is_empty() {
                eprintln!("{:?}", miette::miette!("project {:?}: missing url, skipping sync", name));
                false
            } else if proj.dir.is_empty() {
                eprintln!("{:?}", miette::miette!("project {:?}: missing dir, skipping sync", name));
                false
            } else {
                true
            }
        })
        .collect();

    if projects.is_empty() {
        eprintln!("{:?}", miette::miette!("no projects to sync"));
        return Ok(());
    }

    let chain_pairs: Vec<(String, Vec<String>)> = projects
        .iter()
        .map(|(name, _)| (name.clone(), vec![name.clone()]))
        .collect();
    let projects: std::collections::HashMap<String, crate::compiler::Project> =
        projects.into_iter().collect();
    runner::sync::run_sync_for_projects(projects, chain_pairs)
}
