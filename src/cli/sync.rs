use crate::plan::PlanProject;
use crate::runner;
use std::path::PathBuf;

/// Sync all projects via the TUI.
pub fn run_sync_command(config_arg: Option<PathBuf>) -> miette::Result<()> {
    let config = super::load_config_via(config_arg, crate::compiler::parse_projects_metadata)?;

    let total_project_count = config.projects.len();

    let projects: Vec<(String, PlanProject)> = config
        .projects
        .into_iter()
        .filter(|(name, proj)| {
            let skip_reason = if proj.url.is_empty() && proj.dir.is_empty() {
                Some("missing url and dir")
            } else if proj.url.is_empty() {
                Some("missing url")
            } else if proj.dir.is_empty() {
                Some("missing dir")
            } else {
                None
            };
            if let Some(reason) = skip_reason {
                crate::error::print_diagnostic(&miette::miette!(
                    "project {:?}: {}, skipping sync",
                    name,
                    reason
                ));
                false
            } else {
                true
            }
        })
        .collect();

    if projects.is_empty() {
        if total_project_count == 0 {
            crate::error::print_diagnostic(&miette::miette!("no projects to sync"));
            return Ok(());
        }
        return Err(miette::miette!(
            "all projects were skipped due to missing url or dir"
        ));
    }

    let projects: std::collections::BTreeMap<String, PlanProject> = projects.into_iter().collect();
    runner::sync::run_sync_for_projects(projects)
}
