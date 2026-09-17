//! `kiru sync` command: clones or fast-forward-pulls each repo declared in
//! the selected profile. Reads the default `kiru.toml`, or the one given
//! with `-c`.

use crate::cli::CliError;
use crate::exec::ProjectSync;

pub(crate) fn run_sync_command(
    config_arg: Option<std::path::PathBuf>,
    profile_arg: Option<&str>,
) -> Result<(), CliError> {
    let (profile, _) = crate::cli::resolve_selected_profile(config_arg, profile_arg)?;

    let repos: Vec<ProjectSync> = profile
        .projects
        .into_iter()
        .filter_map(|(name, project)| {
            let skip_reason = if project.url.is_empty() && project.dir.is_empty() {
                Some("missing url and dir")
            } else if project.url.is_empty() {
                Some("missing url")
            } else if project.dir.is_empty() {
                Some("missing dir")
            } else {
                None
            };
            if let Some(reason) = skip_reason {
                eprintln!("Warning: project {name:?}: {}, skipping sync", reason);
                None
            } else {
                Some(ProjectSync {
                    name,
                    url: project.url,
                    dir: project.dir,
                    branch: project.branch,
                })
            }
        })
        .collect();

    if repos.is_empty() {
        eprintln!("Warning: no projects to sync");
        return Ok(());
    }

    crate::exec::run_sync_for_projects(repos).map_err(CliError::from)
}
