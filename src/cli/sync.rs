use super::load_config;
use crate::sync;
use crate::tui::{self, TaskStatus, TuiEvent};
use std::path::PathBuf;
use std::sync::Arc;

pub fn run(config_arg: Option<PathBuf>) -> miette::Result<()> {
    let config = load_config(config_arg)?;

    let project_names: Vec<String> = config.projects.keys().cloned().collect();
    let chain_pairs: Vec<(String, Vec<String>)> = project_names
        .iter()
        .map(|name| (name.clone(), vec![name.clone()]))
        .collect();
    let sanctuary = config.sanctuary.clone();
    let projects = Arc::new(config.projects);

    if let Err(e) = tui::run_tui_with(chain_pairs, move |tx| {
        let sanctuary = sanctuary.clone();
        let projects = Arc::clone(&projects);
        async move {
            for i in 0..project_names.len() {
                crate::tui::send_event(&tx, TuiEvent::UpdateStatus(i, TaskStatus::Running));
            }

            let mut join_handles = Vec::new();

            for (i, proj_name) in project_names.iter().enumerate() {
                let proj = match projects.get(proj_name) {
                    Some(p) => p.clone(),
                    None => {
                        crate::tui::send_event(&tx, TuiEvent::UpdateStatus(i, TaskStatus::Error));
                        continue;
                    }
                };
                let sanctuary = sanctuary.clone();
                let tx_cb = tx.clone();
                let idx = i;

                let handle = tokio::task::spawn_blocking(move || {
                    sync::sync_project_with_callback(&sanctuary, &proj, |line: &str| {
                        crate::tui::send_event(
                            &tx_cb,
                            TuiEvent::AppendOutput(idx, line.to_string()),
                        );
                    })
                });

                join_handles.push((i, handle));
            }

            for (i, handle) in join_handles {
                match handle.await {
                    Ok(Ok(())) => {
                        crate::tui::send_event(&tx, TuiEvent::UpdateStatus(i, TaskStatus::Success));
                    }
                    Ok(Err(e)) => {
                        crate::tui::send_event(
                            &tx,
                            TuiEvent::AppendOutput(i, format!("Error: {}", e)),
                        );
                        crate::tui::send_event(&tx, TuiEvent::UpdateStatus(i, TaskStatus::Error));
                    }
                    Err(e) => {
                        crate::tui::send_event(
                            &tx,
                            TuiEvent::AppendOutput(i, format!("Task panicked: {}", e)),
                        );
                        crate::tui::send_event(&tx, TuiEvent::UpdateStatus(i, TaskStatus::Error));
                    }
                }
            }

            let projects = Arc::clone(&projects);
            let sanctuary = sanctuary.clone();
            let tx = tx.clone();
            let warn_result = tokio::task::spawn_blocking(move || {
                sync::warn_unknown_repos(&sanctuary, Arc::as_ref(&projects))
            })
            .await;

            let has_tasks = !project_names.is_empty();
            match warn_result {
                Ok(Err(e)) => {
                    if has_tasks {
                        crate::tui::send_event(
                            &tx,
                            TuiEvent::AppendOutput(0, format!("Warning: {}", e)),
                        );
                    } else {
                        eprintln!("[kiru] Warning: {}", e);
                    }
                }
                Err(e) => {
                    if has_tasks {
                        crate::tui::send_event(
                            &tx,
                            TuiEvent::AppendOutput(
                                0,
                                format!("Warning: blocking task failed: {}", e),
                            ),
                        );
                    } else {
                        eprintln!("[kiru] Warning: blocking task failed: {}", e);
                    }
                }
                _ => {}
            }

            Ok(())
        }
    }) {
        eprintln!("TUI error: {}", e);
        std::process::exit(1);
    }
    Ok(())
}
