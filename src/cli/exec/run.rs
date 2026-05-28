use super::super::load_config_and_resolve;
use crate::runner::{OutputCallback, Runner};
use crate::tui::{self, TaskStatus, TuiEvent};
use std::path::PathBuf;
use std::sync::Arc;

pub fn run(config_arg: Option<PathBuf>, name: String, project: String) -> miette::Result<()> {
    let config = load_config_and_resolve(config_arg)?;

    if !config.projects.contains_key(&project) {
        return Err(miette::miette!("unknown project: {}", project));
    }

    let project_entry = &config.projects[&project];
    let chains = match project_entry.runs.get(&name) {
        Some(c) => c.clone(),
        None => {
            return Err(miette::miette!(
                "unknown run block '{}' in project '{}'",
                name,
                project
            ));
        }
    };

    let config = Arc::new(config);

    // Build chain pairs: (label, task_names) from config chains
    let chain_pairs: Vec<(String, Vec<String>)> = chains
        .iter()
        .map(|chain| {
            let label = chain.join(" → ");
            let task_names: Vec<String> = chain
                .iter()
                .map(|fn_name| format!("{}({})", fn_name, project))
                .collect();
            (label, task_names)
        })
        .collect();

    tui::run_tui_with(chain_pairs, move |tx| {
        let project = project.clone();
        let config = Arc::clone(&config);
        async move {
            let mut chain_handles = Vec::new();

            let mut base_index = 0;
            for chain in &chains {
                let tx = tx.clone();
                let config = Arc::clone(&config);
                let chain = chain.clone();
                let project = project.clone();
                let start_index = base_index;
                let chain_len = chain.len();

                let handle = tokio::spawn(async move {
                    for (fi, fn_name) in chain.iter().enumerate() {
                        let task_idx = start_index + fi;

                        tui::send_event(&tx, TuiEvent::UpdateStatus(task_idx, TaskStatus::Running));

                        let callback: OutputCallback = Arc::new({
                            let tx = tx.clone();
                            move |line| tui::send_event(&tx, TuiEvent::AppendOutput(task_idx, line))
                        });

                        let mut runner =
                            Runner::from_arc(Arc::clone(&config)).with_output_callback(callback);
                        match runner.execute_fn_call(fn_name, &project) {
                            Ok(()) => {
                                tui::send_event(
                                    &tx,
                                    TuiEvent::UpdateStatus(task_idx, TaskStatus::Success),
                                );
                            }
                            Err(e) => {
                                tui::send_event(
                                    &tx,
                                    TuiEvent::AppendOutput(task_idx, format!("Error: {}", e)),
                                );
                                tui::send_event(
                                    &tx,
                                    TuiEvent::UpdateStatus(task_idx, TaskStatus::Error),
                                );
                                for remaining in fi + 1..chain.len() {
                                    let skip_idx = start_index + remaining;
                                    tui::send_event(
                                        &tx,
                                        TuiEvent::UpdateStatus(skip_idx, TaskStatus::Skipped),
                                    );
                                }
                                return;
                            }
                        }
                    }
                });

                chain_handles.push(handle);
                base_index += chain_len;
            }

            for handle in chain_handles {
                let _ = handle.await;
            }

            Ok(())
        }
    })?;
    Ok(())
}
