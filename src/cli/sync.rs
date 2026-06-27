use super::load_config;
use crate::runner::sync;
use crate::runner::{self, TaskStatus, TuiEvent};
use std::path::PathBuf;
use std::sync::Arc;

pub fn run(config_arg: Option<PathBuf>) -> miette::Result<()> {
    if crate::compiler::is_sanctuary_disabled() {
        return Err(miette::miette!("sync is not available in SANCTUARY=0 mode"));
    }
    let config = load_config(config_arg)?;

    let project_names: Vec<String> = config.projects.keys().cloned().collect();
    let chain_pairs: Vec<(String, Vec<String>)> = project_names
        .iter()
        .map(|name| (name.clone(), vec![name.clone()]))
        .collect();
    let sanctuary = config.sanctuary_path.clone();
    let projects: std::collections::HashMap<String, Arc<crate::compiler::Project>> = config
        .projects
        .into_iter()
        .map(|(k, v)| (k, Arc::new(v)))
        .collect();
    let projects = Arc::new(projects);

    if runner::run_tui_with_sync(chain_pairs, move |tx| {
        let sanctuary = sanctuary.clone();
        let projects = Arc::clone(&projects);
        async move {
            let mut had_errors = false;
            let mut join_handles = Vec::new();

            for (i, proj_name) in project_names.iter().enumerate() {
                let proj = match projects.get(proj_name) {
                    Some(p) => Arc::clone(p),
                    None => {
                        had_errors = true;
                        runner::send_event(&tx, TuiEvent::UpdateStatus(i, TaskStatus::Error));
                        continue;
                    }
                };
                let sanctuary = sanctuary.clone();
                let tx_cb = tx.clone();
                let idx = i;

                let handle = tokio::task::spawn_blocking(move || {
                    runner::send_event(&tx_cb, TuiEvent::UpdateStatus(idx, TaskStatus::Running));
                    sync::sync_project_with_callback(&sanctuary, &proj, |line: &str| {
                        runner::send_event(&tx_cb, TuiEvent::AppendOutput(idx, line.to_string()));
                    })
                });

                join_handles.push((i, handle));
            }

            for (i, handle) in join_handles {
                match handle.await {
                    Ok(Ok(())) => {
                        runner::send_event(&tx, TuiEvent::UpdateStatus(i, TaskStatus::Success));
                    }
                    Ok(Err(e)) => {
                        had_errors = true;
                        runner::send_event(&tx, TuiEvent::AppendOutput(i, format!("Error: {}", e)));
                        runner::send_event(&tx, TuiEvent::UpdateStatus(i, TaskStatus::Error));
                    }
                    Err(e) => {
                        had_errors = true;
                        runner::send_event(
                            &tx,
                            TuiEvent::AppendOutput(i, format!("Task panicked: {}", e)),
                        );
                        runner::send_event(&tx, TuiEvent::UpdateStatus(i, TaskStatus::Error));
                    }
                }
            }

            if had_errors {
                Err(miette::miette!("One or more projects failed to sync"))
            } else {
                Ok(())
            }
        }
    })
    .is_err()
    {
        std::process::exit(1);
    }
    Ok(())
}
