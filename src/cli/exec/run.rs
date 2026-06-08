use super::super::load_config_and_resolve;
use crate::runner::Output;
use crate::runner::executor::ExecContext;
use crate::tui::{self, TaskStatus, TuiEvent};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

fn run_project_chains(
    config: Arc<crate::config::Config>,
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
    tui::run_tui_with(chain_pairs, move |tx| {
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
                    let mut output = Output::Callback(Arc::new(cb));

                    let project_entry = config.projects.get(&project).unwrap();
                    let mut ctx = ExecContext::new(&config, Some(project_entry), &mut output);

                    for (fi, fn_name) in chain.iter().enumerate() {
                        let task_idx = start_index + fi;
                        current_task.store(task_idx, Ordering::Relaxed);
                        tui::send_event(&tx, TuiEvent::UpdateStatus(task_idx, TaskStatus::Running));

                        let fn_body = match project_entry.functions.get(fn_name) {
                            Some(b) => b.clone(),
                            None => {
                                tui::send_event(
                                    &tx,
                                    TuiEvent::AppendOutput(
                                        task_idx,
                                        format!("Error: unknown function '{}'", fn_name),
                                    ),
                                );
                                tui::send_event(
                                    &tx,
                                    TuiEvent::UpdateStatus(task_idx, TaskStatus::Error),
                                );
                                for remaining in fi + 1..chain.len() {
                                    tui::send_event(
                                        &tx,
                                        TuiEvent::UpdateStatus(
                                            start_index + remaining,
                                            TaskStatus::Skipped,
                                        ),
                                    );
                                }
                                return Err(());
                            }
                        };

                        match ctx.exec_fn_body(&fn_body) {
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
                Err(miette::miette!("one or more chain tasks failed"))
            } else {
                Ok(())
            }
        }
    })?;
    Ok(())
}

fn run_standalone_chains(
    config: Arc<crate::config::Config>,
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

    tui::run_tui_with(chain_pairs, move |tx| {
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
                    let mut output = Output::Callback(Arc::new(cb));

                    let mut ctx = ExecContext::new(&config, None, &mut output);

                    for (fi, fn_name) in chain.iter().enumerate() {
                        let task_idx = start_index + fi;
                        current_task.store(task_idx, Ordering::Relaxed);
                        tui::send_event(&tx, TuiEvent::UpdateStatus(task_idx, TaskStatus::Running));

                        let fn_body = match config.functions.get(fn_name) {
                            Some(b) => b.clone(),
                            None => {
                                tui::send_event(
                                    &tx,
                                    TuiEvent::AppendOutput(
                                        task_idx,
                                        format!("Error: unknown function '{}'", fn_name),
                                    ),
                                );
                                tui::send_event(
                                    &tx,
                                    TuiEvent::UpdateStatus(task_idx, TaskStatus::Error),
                                );
                                for remaining in fi + 1..chain.len() {
                                    tui::send_event(
                                        &tx,
                                        TuiEvent::UpdateStatus(
                                            start_index + remaining,
                                            TaskStatus::Skipped,
                                        ),
                                    );
                                }
                                return Err(());
                            }
                        };

                        match ctx.exec_fn_body(&fn_body) {
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
                Err(miette::miette!("one or more chain tasks failed"))
            } else {
                Ok(())
            }
        }
    })?;
    Ok(())
}

pub fn run(
    config_arg: Option<PathBuf>,
    name: String,
    project: Option<String>,
) -> miette::Result<()> {
    let config = load_config_and_resolve(config_arg)?;

    let is_standalone = crate::config::is_sanctuary_disabled() && config.projects.is_empty();

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
