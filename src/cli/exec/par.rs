use super::super::load_config_and_resolve;
use crate::runner::{OutputCallback, Runner};
use crate::tui::{self, TaskStatus, TuiEvent};
use std::path::PathBuf;
use std::sync::Arc;

pub fn run(
    config_arg: Option<PathBuf>,
    name: String,
    project: String,
    plain: bool,
) -> miette::Result<()> {
    let config = load_config_and_resolve(config_arg)?;

    if !config.projects.contains_key(&project) {
        return Err(miette::miette!("unknown project: {}", project));
    }

    let project_entry = &config.projects[&project];
    let fns = match project_entry.pars.get(&name) {
        Some(f) => f.clone(),
        None => {
            return Err(miette::miette!(
                "unknown par '{}' in project '{}'",
                name,
                project
            ));
        }
    };

    if plain {
        let mut runner = Runner::new(config);
        runner
            .run_par(&name, &project)
            .map_err(|e| miette::miette!("{}", e))?;
    } else {
        let config = Arc::new(config);

        let task_names: Vec<String> = fns
            .iter()
            .map(|fn_name| format!("{}({})", fn_name, project))
            .collect();

        use std::sync::atomic::{AtomicBool, Ordering};

        let had_error = Arc::new(AtomicBool::new(false));

        tui::run_tui_with(task_names, move |tx| {
            let project = project.clone();
            let had_error = Arc::clone(&had_error);
            async move {
                for task_idx in 0..fns.len() {
                    tui::send_event(&tx, TuiEvent::UpdateStatus(task_idx, TaskStatus::Running));
                }

                let mut join_handles = Vec::new();

                for (task_idx, fn_name) in fns.iter().enumerate() {
                    let tx = tx.clone();
                    let config = Arc::clone(&config);
                    let fn_name = fn_name.clone();
                    let project = project.clone();
                    let had_error = Arc::clone(&had_error);

                    let handle = tokio::spawn(async move {
                        let callback: OutputCallback = Arc::new({
                            let tx = tx.clone();
                            move |line| tui::send_event(&tx, TuiEvent::AppendOutput(task_idx, line))
                        });

                        let mut runner = Runner::from_arc(config).with_output_callback(callback);
                        match runner.execute_fn_call(&fn_name, &project) {
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
                            }
                        }
                    });

                    join_handles.push(handle);
                }

                for handle in join_handles {
                    if handle.await.is_err() {
                        had_error.store(true, Ordering::Relaxed);
                    }
                }

                if had_error.load(Ordering::Relaxed) {
                    return Err(miette::miette!("one or more parallel tasks failed"));
                }

                Ok(())
            }
        })?;
    }
    Ok(())
}
