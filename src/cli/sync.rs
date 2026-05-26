use super::load_config;
use crate::sync;
use crate::tui::{self, TaskStatus, TuiApp, TuiEvent};
use std::io;
use std::path::PathBuf;

pub fn run(config_arg: Option<PathBuf>, plain: bool) -> miette::Result<()> {
    let config = load_config(config_arg)?;

    if plain {
        let mut stdout = io::stdout();
        sync::sync_all(&config, &mut stdout).map_err(|e| miette::miette!("{}", e))?;
    } else {
        let rt = tokio::runtime::Runtime::new().map_err(|e| miette::miette!("{}", e))?;
        rt.block_on(async {
            let project_names: Vec<String> = config.projects.keys().cloned().collect();
            let mut model = tui::Model::new();

            for proj_name in &project_names {
                model.add_task(proj_name.clone());
            }

            let app = TuiApp::new(model);
            let tx = app.get_sender();

            let tx_clone = tx.clone();
            let sanctuary = config.sanctuary.clone();
            let projects = config.projects.clone();
            tokio::spawn(async move {
                for (i, proj_name) in project_names.iter().enumerate() {
                    crate::tui::send_event(
                        &tx_clone,
                        TuiEvent::UpdateStatus(i, TaskStatus::Running),
                    );

                    if let Some(proj) = projects.get(proj_name) {
                        let proj = proj.clone();
                        let sanctuary = sanctuary.clone();
                        let tx = tx_clone.clone();
                        let result = tokio::task::spawn_blocking(move || {
                            sync::sync_project_with_callback(&sanctuary, &proj, |line: &str| {
                                crate::tui::send_event(
                                    &tx,
                                    TuiEvent::AppendOutput(i, line.to_string()),
                                );
                            })
                        })
                        .await;

                        match result {
                            Ok(Ok(())) => {
                                crate::tui::send_event(
                                    &tx_clone,
                                    TuiEvent::UpdateStatus(i, TaskStatus::Success),
                                );
                            }
                            Ok(Err(e)) => {
                                crate::tui::send_event(
                                    &tx_clone,
                                    TuiEvent::AppendOutput(i, format!("Error: {}", e)),
                                );
                                crate::tui::send_event(
                                    &tx_clone,
                                    TuiEvent::UpdateStatus(i, TaskStatus::Error),
                                );
                            }
                            Err(e) => {
                                crate::tui::send_event(
                                    &tx_clone,
                                    TuiEvent::AppendOutput(i, format!("Task failed: {}", e)),
                                );
                                crate::tui::send_event(
                                    &tx_clone,
                                    TuiEvent::UpdateStatus(i, TaskStatus::Error),
                                );
                            }
                        }
                    } else {
                        crate::tui::send_event(
                            &tx_clone,
                            TuiEvent::UpdateStatus(i, TaskStatus::Error),
                        );
                    }
                }

                let projects_for_warn = projects.clone();
                let sanctuary = sanctuary.clone();
                let tx_clone = tx_clone.clone();
                let warn_result = tokio::task::spawn_blocking(move || {
                    sync::warn_unknown_repos(&sanctuary, &projects_for_warn)
                })
                .await;

                let has_tasks = !project_names.is_empty();
                match warn_result {
                    Ok(Err(e)) => {
                        if has_tasks {
                            crate::tui::send_event(
                                &tx_clone,
                                TuiEvent::AppendOutput(0, format!("Warning: {}", e)),
                            );
                        } else {
                            eprintln!("[sro] Warning: {}", e);
                        }
                    }
                    Err(e) => {
                        if has_tasks {
                            crate::tui::send_event(
                                &tx_clone,
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
            });

            if let Err(e) = app.run().await {
                eprintln!("TUI error: {}", e);
                std::process::exit(1);
            }
        });
    }
    Ok(())
}
