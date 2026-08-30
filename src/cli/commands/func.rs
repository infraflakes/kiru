use crate::cli::load_config;
use crate::exec::colors;
use crate::exec::{Executor, OutputCallback};
use std::io::{self, Write};
use std::path::PathBuf;
use std::sync::Arc;

pub fn execute_function(
    config_arg: Option<PathBuf>,
    name: String,
    project: Option<String>,
) -> Result<(), String> {
    let config = load_config(config_arg)?;

    let callback: OutputCallback = Arc::new(|line| {
        let mut stdout_locked = io::stdout().lock();
        colors::write_colored_line(&line, &mut stdout_locked);
        let _ = writeln!(stdout_locked);
    });

    match project {
        Some(project_name) => {
            let mut executor = Executor::new(Arc::new(config), callback);
            executor
                .execute_fn_call(&name, &project_name)
                .map_err(|e| e.to_string())?;
            Ok(())
        }
        None => Err(format!(
            "must specify a project to run function '{}' in",
            name
        )),
    }
}
