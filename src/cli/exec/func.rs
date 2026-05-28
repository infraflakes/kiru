use super::super::load_config_and_resolve;
use crate::runner::{OutputCallback, Runner};
use std::io::{self, Write};
use std::path::PathBuf;
use std::sync::Arc;

pub fn run(config_arg: Option<PathBuf>, name: String, project: String) -> miette::Result<()> {
    let config = load_config_and_resolve(config_arg)?;

    if !config.projects.contains_key(&project) {
        return Err(miette::miette!("unknown project: {}", project));
    }

    if !config.projects[&project].functions.contains_key(&name) {
        return Err(miette::miette!(
            "unknown function {} in project {}",
            name,
            project
        ));
    }

    let callback: OutputCallback = Arc::new(|line| {
        let mut out = io::stdout().lock();
        let _ = crate::tui::render::write_colored_line(&line, &mut out);
        let _ = writeln!(out);
    });

    let mut runner = Runner::new(config).with_output_callback(callback);
    runner
        .execute_fn_call(&name, &project)
        .map_err(|e| miette::miette!("{}", e))
}
