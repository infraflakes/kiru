//! `kiru sync` command: clones or fast-forward-pulls each project declared in
//! `kiru.toml` into its directory. Reads the default `kiru.toml`, or the one
//! given with `-c`.

use crate::cli::CliError;
use crate::cli::get_toml_path;
use crate::cli::kiru_toml;
use crate::exec::ProjectSync;

pub(crate) fn run_sync_command(config_arg: Option<std::path::PathBuf>) -> Result<(), CliError> {
    let toml_path = get_toml_path(config_arg);
    if !toml_path.exists() {
        return Err(CliError::message(format!(
            "kiru sync requires kiru.toml (not found at {})",
            toml_path.display()
        )));
    }
    let mut toml = kiru_toml::load_kiru_toml_at(&toml_path).map_err(CliError::message)?;
    kiru_toml::expand_project_dirs(&mut toml);

    let projects: Vec<ProjectSync> = toml
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

    if projects.is_empty() {
        eprintln!("Warning: no projects to sync");
        return Ok(());
    }

    crate::exec::run_sync_for_projects(projects).map_err(CliError::from)
}
