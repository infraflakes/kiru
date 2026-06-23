use super::super::load_config_and_resolve;
use crate::runner::{OutputCallback, Runner};
use std::io::{self, Write};
use std::path::PathBuf;
use std::sync::Arc;

pub fn execute_function(
    config_arg: Option<PathBuf>,
    name: String,
    project: Option<String>,
) -> miette::Result<()> {
    let config = load_config_and_resolve(config_arg)?;

    let is_standalone = crate::config::is_sanctuary_disabled() && config.projects.is_empty();

    let callback: OutputCallback = Arc::new(|line| {
        let mut out = io::stdout().lock();
        crate::colors::write_colored_line(&line, &mut out);
        let _ = writeln!(out);
    });

    match project {
        Some(ref proj) => {
            if is_standalone {
                return Err(miette::miette!(
                    "Project name cannot be specified when SANCTUARY=0 is set",
                ));
            }
            if !config.projects.contains_key(proj) {
                return Err(miette::miette!("unknown project: {}", proj));
            }
            if !config.projects[proj].functions.contains_key(&name) {
                return Err(miette::miette!(
                    "unknown function {} in project {}",
                    name,
                    proj
                ));
            }

            let mut runner = Runner::new(config).with_output_callback(callback);
            runner
                .execute_fn_call(&name, proj)
                .map_err(|e| miette::miette!("{}", e))
        }
        None => {
            if !config.functions.contains_key(&name) {
                return Err(miette::miette!(
                    "unknown function {} (no project specified, and no top-level function with that name)",
                    name
                ));
            }

            let mut runner = Runner::new(config).with_output_callback(callback);
            runner
                .execute_standalone_fn(&name)
                .map_err(|e| miette::miette!("{}", e))
        }
    }
}
