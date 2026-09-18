//! `kiru run` command: executes a named run block by resolving its chain
//! of project-function calls through the TUI. The IR comes from the
//! profile's `output`; the profile supplies shell, timeout, and projects.

use crate::cli::CliError;
use crate::exec;
use std::path::PathBuf;
use std::sync::Arc;

pub(crate) fn execute_run_block(
    config_arg: Option<PathBuf>,
    profile_arg: Option<&str>,
    name: String,
) -> Result<(), CliError> {
    let (profile, _) = crate::cli::resolve_selected_profile(config_arg, profile_arg)?;

    // The profile's output is the compiled form of its source; the user
    // compiles explicitly, run never compiles.
    let program = match crate::cli::load_program(&profile.output) {
        Ok(Some(program)) => Arc::new(program),
        Ok(None) => {
            return Err(CliError::message(format!(
                "no compiled program at {} (run `kiru compile`)",
                profile.output.display()
            )));
        }
        Err(error) => return Err(CliError::message(error.message())),
    };
    if !program.runs.contains_key(&name) {
        return Err(CliError::message(format!("unknown run block '{}'", name)));
    }

    let invocation_cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/"));
    exec::execute_run(
        program,
        &name,
        crate::cli::exec_projects(&profile),
        invocation_cwd,
        crate::cli::profile_shell(&profile),
        crate::cli::profile_timeout(&profile),
    )
    .map_err(CliError::from)
}
