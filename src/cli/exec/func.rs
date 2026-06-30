use super::super::load_config;
use crate::runner::colors;
use crate::runner::{OutputCallback, Runner};
use std::io::{self, Write};
use std::path::PathBuf;
use std::sync::Arc;

/// Execute a single function, optionally scoped to a project, and print its
/// output to stdout.
pub fn execute_function(
    config_arg: Option<PathBuf>,
    name: String,
    project: Option<String>,
) -> miette::Result<()> {
    let config = load_config(config_arg)?;

    let callback: OutputCallback = Arc::new(|line| {
        let mut stdout_locked = io::stdout().lock();
        colors::write_colored_line(&line, &mut stdout_locked);
        let _ = writeln!(stdout_locked);
    });

    match project {
        Some(ref project_name) => {
            if !config.projects.contains_key(project_name) {
                return Err(miette::miette!("unknown project: {}", project_name));
            }
            if !config.projects[project_name].functions.contains_key(&name) {
                return Err(miette::miette!(
                    "unknown function {} in project {}",
                    name,
                    project_name
                ));
            }

            let mut runner = Runner::new(Arc::new(config)).with_output_callback(callback);
            runner.execute_fn_call(&name, project_name)?;
            Ok(())
        }
        None => Err(miette::miette!(
            "must specify a project to run function '{}' in",
            name
        )),
    }
}
