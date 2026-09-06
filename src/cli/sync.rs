//! `kiru sync` command: clones or fast-forward-pulls each repo declared in
//! `kiru.toml` into its directory. Reads the default `kiru.toml`, or the one
//! given with `-c`.

use crate::cli::CliError;
use crate::cli::get_toml_path;
use crate::cli::kiru_toml;
use crate::exec::RepoSync;

pub(crate) fn run_sync_command(config_arg: Option<std::path::PathBuf>) -> Result<(), CliError> {
    let toml_path = get_toml_path(config_arg);
    if !toml_path.exists() {
        return Err(CliError::message(format!(
            "kiru sync requires kiru.toml (not found at {})",
            toml_path.display()
        )));
    }
    let mut toml = kiru_toml::load_kiru_toml_at(&toml_path).map_err(CliError::message)?;
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
        })
        .collect();

    if repos.is_empty() {
        eprintln!("Warning: no projects to sync");
        return Ok(());
    }

    crate::exec::run_sync_for_projects(repos).map_err(CliError::from)
}
