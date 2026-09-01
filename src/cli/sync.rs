//! `kiru sync` command: reads `kiru.toml` directly and clones or
//! fast-forward-pulls each project into its declared directory.

use crate::cli::kiru_toml;
use crate::exec::sync::RepoSync;

pub(crate) fn run_sync_command(config_arg: Option<std::path::PathBuf>) -> Result<(), String> {
    // When a config_arg is given, it's a kirufile path, but sync needs the toml.
    // For now, sync always reads the canonical toml location.
    let _ = config_arg;

    let mut toml = kiru_toml::load_kiru_toml()?;
    kiru_toml::expand_repo_dirs(&mut toml);

    let repos: Vec<RepoSync> = toml
        .repos
        .into_iter()
        .filter(|repo| {
            let skip_reason = if repo.url.is_empty() && repo.dir.is_empty() {
                Some("missing url and dir")
            } else if repo.url.is_empty() {
                Some("missing url")
            } else if repo.dir.is_empty() {
                Some("missing dir")
            } else {
                None
            };
            if let Some(reason) = skip_reason {
                eprintln!(
                    "Warning: project {:?}: {}, skipping sync",
                    repo.name, reason
                );
                false
            } else {
                true
            }
        })
        .map(|repo| RepoSync {
            name: repo.name,
            url: repo.url,
            dir: repo.dir,
            branch: repo.branch,
            strategy: repo.strategy,
        })
        .collect();

    if repos.is_empty() {
        eprintln!("Warning: no projects to sync");
        return Ok(());
    }

    crate::exec::sync::run_sync_for_projects(repos)
}
