//! `kiru fn` command: executes a single project function directly,
//! streaming colored output to stdout (no TUI).

use crate::cli::kiru_toml;
use crate::cli::load_config;
use crate::exec::colors;
use crate::exec::error::RuntimeError;
use crate::exec::{Executor, OutputCallback};
use std::io::{self, Write};
use std::path::PathBuf;
use std::sync::Arc;

pub(crate) fn execute_function(
    config_arg: Option<PathBuf>,
    name: String,
    project: Option<String>,
) -> Result<(), String> {
    let config = load_config(config_arg)?;

    let toml = kiru_toml::load_kiru_toml()?;
    let mut toml_expanded = toml.clone();
    kiru_toml::expand_repo_dirs(&mut toml_expanded);

    let callback: OutputCallback = Arc::new(|line| {
        let mut stdout_locked = io::stdout().lock();
        colors::write_colored_line(&line, &mut stdout_locked);
        let _ = writeln!(stdout_locked);
    });

    let invocation_cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("/"));
    let timeout = toml.timeout.map(std::time::Duration::from_secs);

    match project {
        Some(project_name) => {
            let cwd = toml_expanded
                .repos
                .iter()
                .find(|r| r.name == project_name && !r.dir.is_empty())
                .map(|r| PathBuf::from(&r.dir))
                .unwrap_or_else(|| invocation_cwd.clone());

            let mut executor = Executor::new(Arc::new(config), toml.shell, timeout, callback);
            executor
                .execute_fn_call(&name, &project_name, cwd)
                .map_err(|e| match e {
                    // Timeout error already emitted via OutputCallback inside
                    // ExecContext::run_live, return empty to suppress duplicate.
                    RuntimeError::Timeout { .. } => String::new(),
                    other => other.to_string(),
                })?;
            Ok(())
        }
        None => Err(format!(
            "must specify a project to run function '{name}' in"
        )),
    }
}
