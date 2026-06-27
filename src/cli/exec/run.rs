use super::super::load_config_and_resolve;
use crate::runner::Runner;
use crate::runner::tui::{self, TaskStatus, TuiEvent};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

fn run_project_chains(
    config: Arc<crate::compiler::Sanctuary>,
    project: &str,
    chains: Vec<Vec<String>>,
) -> miette::Result<()> {
    let (chain_pairs, chain_tasks): (Vec<_>, Vec<_>) = chains
        .iter()
        .map(|chain| {
            let label = chain.join(" → ");
            let task_names: Vec<String> = chain
                .iter()
                .map(|fn_name| format!("{}({})", fn_name, project))
                .collect();
            ((label, task_names), chain.clone())
        })
        .unzip();

    let project = project.to_string();
    tui::run_tui_with_run(chain_pairs, move |tx| {
        let project = project.clone();
        let config = Arc::clone(&config);
        async move {
            let mut chain_handles = Vec::new();

            let mut base_index = 0;
            for chain in &chain_tasks {
                let tx = tx.clone();
                let config = Arc::clone(&config);
                let chain = chain.clone();
                let project = project.clone();
                let start_index = base_index;
                let chain_len = chain.len();

                let handle = tokio::task::spawn_blocking(move || -> Result<(), ()> {
                    let current_task = Arc::new(AtomicUsize::new(0));
                    let cb = {
                        let tx = tx.clone();
                        let current_task = Arc::clone(&current_task);
                        move |line: String| {
                            let idx = current_task.load(Ordering::Relaxed);
                            tui::send_event(&tx, TuiEvent::AppendOutput(idx, line))
                        }
                    };
                    let mut runner =
                        Runner::new((*config).clone()).with_output_callback(Arc::new(cb));

                    for (fi, fn_name) in chain.iter().enumerate() {
                        let task_idx = start_index + fi;
                        current_task.store(task_idx, Ordering::Relaxed);
                        tui::send_event(&tx, TuiEvent::UpdateStatus(task_idx, TaskStatus::Running));

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
                                return Err(());
                            }
                        }
                    }

                    Ok(())
                });

                chain_handles.push(handle);
                base_index += chain_len;
            }

            let mut any_err = false;
            for handle in chain_handles {
                match handle.await {
                    Ok(Ok(())) => {}
                    _ => any_err = true,
                }
            }

            if any_err {
                Err(miette::miette!("One or more chain tasks failed"))
            } else {
                Ok(())
            }
        }
    })?;
    Ok(())
}

fn run_standalone_chains(
    config: Arc<crate::compiler::Sanctuary>,
    chains: Vec<Vec<String>>,
) -> miette::Result<()> {
    let (chain_pairs, chain_tasks): (Vec<_>, Vec<_>) = chains
        .iter()
        .map(|chain| {
            let label = chain.join(" → ");
            let task_names: Vec<String> = chain.to_vec();
            ((label, task_names), chain.clone())
        })
        .unzip();

    tui::run_tui_with_run(chain_pairs, move |tx| {
        let config = Arc::clone(&config);
        async move {
            let mut chain_handles = Vec::new();

            let mut base_index = 0;
            for chain in &chain_tasks {
                let tx = tx.clone();
                let config = Arc::clone(&config);
                let chain = chain.clone();
                let start_index = base_index;
                let chain_len = chain.len();

                let handle = tokio::task::spawn_blocking(move || -> Result<(), ()> {
                    let current_task = Arc::new(AtomicUsize::new(0));
                    let cb = {
                        let tx = tx.clone();
                        let current_task = Arc::clone(&current_task);
                        move |line: String| {
                            let idx = current_task.load(Ordering::Relaxed);
                            tui::send_event(&tx, TuiEvent::AppendOutput(idx, line))
                        }
                    };
                    let mut runner =
                        Runner::new((*config).clone()).with_output_callback(Arc::new(cb));

                    for (fi, fn_name) in chain.iter().enumerate() {
                        let task_idx = start_index + fi;
                        current_task.store(task_idx, Ordering::Relaxed);
                        tui::send_event(&tx, TuiEvent::UpdateStatus(task_idx, TaskStatus::Running));

                        match runner.execute_standalone_fn(fn_name) {
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
                                return Err(());
                            }
                        }
                    }

                    Ok(())
                });

                chain_handles.push(handle);
                base_index += chain_len;
            }

            let mut any_err = false;
            for handle in chain_handles {
                match handle.await {
                    Ok(Ok(())) => {}
                    _ => any_err = true,
                }
            }

            if any_err {
                Err(miette::miette!("One or more chain tasks failed"))
            } else {
                Ok(())
            }
        }
    })?;
    Ok(())
}

pub fn execute_run_block(
    config_arg: Option<PathBuf>,
    name: String,
    project: Option<String>,
) -> miette::Result<()> {
    let config = load_config_and_resolve(config_arg)?;

    let is_standalone = crate::compiler::is_sanctuary_disabled();

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

            let project_entry = &config.projects[proj];
            let chains = match project_entry.runs.get(&name) {
                Some(c) => c.clone(),
                None => {
                    return Err(miette::miette!(
                        "unknown run block '{}' in project '{}'",
                        name,
                        proj
                    ));
                }
            };

            run_project_chains(Arc::new(config), proj, chains)
        }
        None => {
            let chains = match config.runs.get(&name) {
                Some(c) => c.clone(),
                None => {
                    return Err(miette::miette!(
                        "unknown run block '{}' (no project specified, and no top-level run with that name)",
                        name
                    ));
                }
            };

            run_standalone_chains(Arc::new(config), chains)
        }
    }
}
