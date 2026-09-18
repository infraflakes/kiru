//! `kiru sync` command: clones or fast-forward-pulls each repo declared in
//! the selected profile. Reads the default `kiru.toml`, or the one given
//! with `-c`.

use crate::cli::CliError;

pub(crate) fn run_sync_command(
    config_arg: Option<std::path::PathBuf>,
    profile_arg: Option<&str>,
) -> Result<(), CliError> {
    let (profile, _) = crate::cli::resolve_selected_profile(config_arg, profile_arg)?;

    let repos = crate::cli::sync_projects(&profile);
    if repos.is_empty() {
        eprintln!("Warning: no projects to sync");
        return Ok(());
    }

    crate::exec::run_sync_for_projects(repos).map_err(CliError::from)
}
