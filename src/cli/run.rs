//! `kiru run` command: executes a named run block by resolving its chain
//! of project-function calls through the TUI. The IR comes from the
//! profile's `output`; the profile supplies shell, timeout, and projects.

use crate::cli::CliError;
use crate::exec;
use crate::exec::ProjectExec;
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

pub(crate) fn execute_run_block(
    config_arg: Option<PathBuf>,
    profile_arg: Option<&str>,
    name: String,
) -> Result<(), CliError> {
    let (profile, _) = crate::cli::resolve_selected_profile(config_arg, profile_arg)?;

    // The IR is the compiled form of the profile's source; the user
    // compiles explicitly, run never compiles.
    let ir = crate::cli::load_config(&profile.output).map_err(CliError::message)?;

    let mut repos = BTreeMap::new();
    for (project_name, project) in &profile.projects {
        if !project.dir.is_empty() {
            repos.insert(
                project_name.clone(),
                ProjectExec {
                    dir: PathBuf::from(&project.dir),
                    direnv: project.direnv,
                },
            );
        }
    }

    let body = match ir.runs.get(&name) {
        Some(body) => body.clone(),
        None => {
            return Err(CliError::message(format!("unknown run block '{}'", name)));
        }
    };
    let plan = ir.run_plan(&name);

    let invocation_cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/"));
    let timeout = profile.timeout.map(std::time::Duration::from_secs);

    exec::execute_run(
        body,
        plan,
        Arc::new(repos),
        invocation_cwd,
        profile.shell.unwrap_or_else(|| "sh".to_string()),
        timeout,
    )
    .map_err(CliError::from)
}
