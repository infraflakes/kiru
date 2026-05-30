use super::super::load_config_and_resolve;
use crate::runner::Output;
use crate::runner::executor::ExecContext;
use crate::tui::{self, TaskStatus, TuiEvent};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

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
                    let mut ctx = ExecContext::new(&config, project_entry, &mut output);

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
