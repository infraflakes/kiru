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

    // Pre-compute base index in flat task list for each chain
    let mut base_indices: Vec<usize> = Vec::with_capacity(chains.len());
    let mut offset = 0;
    for chain in &chains {
        base_indices.push(offset);
        offset += chain.len();
    }

    // Build task names
    let mut task_names: Vec<String> = Vec::new();
    for (ci, chain) in chains.iter().enumerate() {
        for fn_name in chain {
            let label = if chains.len() > 1 {
                format!("{}({}) chain{}", fn_name, project, ci + 1)
            } else {
                format!("{}({})", fn_name, project)
            };
            task_names.push(label);
        }
    }

    use std::sync::atomic::{AtomicBool, Ordering};

    let had_error = Arc::new(AtomicBool::new(false));

    let chains_arc = Arc::new(chains);
    let base_indices_arc = Arc::new(base_indices);

    tui::run_tui_with(task_names, move |tx| {
        let project = project.clone();
        let had_error = Arc::clone(&had_error);
        let chains = Arc::clone(&chains_arc);
        let base_indices = Arc::clone(&base_indices_arc);
        async move {
            let mut chain_handles = Vec::new();

            for (ci, chain) in chains.iter().enumerate() {
                let tx = tx.clone();
                let config = Arc::clone(&config);
                let chain = chain.clone();
                let project = project.clone();
                let had_error = Arc::clone(&had_error);
                let base_index = base_indices[ci];

                let handle = tokio::spawn(async move {
                    for (fi, fn_name) in chain.iter().enumerate() {
                        let task_idx = base_index + fi;

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
                                had_error.store(true, Ordering::Relaxed);
                                tui::send_event(
                                    &tx,
                                    TuiEvent::AppendOutput(task_idx, format!("Error: {}", e)),
                                );
                                tui::send_event(
                                    &tx,
                                    TuiEvent::UpdateStatus(task_idx, TaskStatus::Error),
                                );
                                for remaining in fi + 1..chain.len() {
                                    let skip_idx = base_index + remaining;
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
            }

            for handle in chain_handles {
                if handle.await.is_err() {
                    had_error.store(true, Ordering::Relaxed);
                }
            }

            if had_error.load(Ordering::Relaxed) {
                return Err(miette::miette!("one or more tasks failed"));
            }

            Ok(())
        }
    })?;
    Ok(())
}
