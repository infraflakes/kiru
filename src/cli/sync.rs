use super::load_config;
use crate::sync;
use crate::tui::{self, TaskStatus, TuiEvent};
use std::io;
use std::path::PathBuf;
use std::sync::Arc;

pub fn run(config_arg: Option<PathBuf>, plain: bool) -> miette::Result<()> {
    let config = load_config(config_arg)?;

    if plain {
        let mut stdout = io::stdout();
        sync::sync_all(&config, &mut stdout).map_err(|e| miette::miette!("{}", e))?;
    } else {
        let project_names: Vec<String> = config.projects.keys().cloned().collect();
        let task_names = project_names.clone();
        let sanctuary = config.sanctuary.clone();
        let projects = Arc::new(config.projects);

        if let Err(e) = tui::run_tui_with(task_names, move |tx| {
            let sanctuary = sanctuary.clone();
            let projects = Arc::clone(&projects);
            async move {
                for (i, proj_name) in project_names.iter().enumerate() {
                    crate::tui::send_event(&tx, TuiEvent::UpdateStatus(i, TaskStatus::Running));

                    if let Some(proj) = projects.get(proj_name) {
                        let proj = proj.clone();
                        let sanctuary = sanctuary.clone();
                        let tx_cb = tx.clone();
                        let result = tokio::task::spawn_blocking(move || {
                            sync::sync_project_with_callback(&sanctuary, &proj, |line: &str| {
                                crate::tui::send_event(
                                    &tx_cb,
                                    TuiEvent::AppendOutput(i, line.to_string()),
                                );
                            })
                        })
                        .await;

                        match result {
                            Ok(Ok(())) => {
                                crate::tui::send_event(
                                    &tx,
                                    TuiEvent::UpdateStatus(i, TaskStatus::Success),
                                );
                            }
                            Ok(Err(e)) => {
                                crate::tui::send_event(
                                    &tx,
                                    TuiEvent::AppendOutput(i, format!("Error: {}", e)),
                                );
                                crate::tui::send_event(
                                    &tx,
                                    TuiEvent::UpdateStatus(i, TaskStatus::Error),
                                );
                            }
                            Err(e) => {
                                crate::tui::send_event(
                                    &tx,
                                    TuiEvent::AppendOutput(i, format!("Task failed: {}", e)),
                                );
                                crate::tui::send_event(
                                    &tx,
                                    TuiEvent::UpdateStatus(i, TaskStatus::Error),
                                );
                            }
                        }
                    } else {
                        crate::tui::send_event(&tx, TuiEvent::UpdateStatus(i, TaskStatus::Error));
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
                            eprintln!("[sro] Warning: {}", e);
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
                            eprintln!("[sro] Warning: blocking task failed: {}", e);
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
    }
    Ok(())
}
