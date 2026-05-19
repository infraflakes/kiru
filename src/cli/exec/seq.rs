use super::super::load_config_and_resolve;
use crate::runner::{OutputCallback, Runner};
use crate::tui::{self, Model, TaskStatus, TuiApp, TuiEvent};
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

    let fns = match config.projects[&project].seqs.get(&name) {
        Some(fns) => fns.clone(),
        None => {
            return Err(miette::miette!(
                "unknown sequence {} in project {}",
                name,
                project
            ));
        }
    };

    if plain {
        let mut runner = Runner::new(config);
        runner
            .run_seq(&name, &project)
            .map_err(|e| miette::miette!("{}", e))?;
    } else {
        let rt = tokio::runtime::Runtime::new().map_err(|e| miette::miette!("{}", e))?;
        let result: miette::Result<()> = rt.block_on(async {
            let config = Arc::new(config);

            let mut model = Model::new("seq".to_string(), format!("{} ({})", name, project));

            for fn_name in &fns {
                model.add_task(format!("{}({})", fn_name, project));
            }

            let app = TuiApp::new(model);
            let tx = app.get_sender();

            let config_arc = Arc::clone(&config);
            let tx_clone = tx.clone();
            let project_clone = project.clone();
            let handle = tokio::spawn(async move {
                for (task_idx, fn_name) in fns.iter().enumerate() {
                    tui::send_event(
                        &tx_clone,
                        TuiEvent::UpdateStatus(task_idx, TaskStatus::Running),
                    );

                    let tx_clone_for_callback = tx_clone.clone();
                    let task_idx_for_callback = task_idx;

                    let callback: OutputCallback = Arc::new(move |line| {
                        tui::send_event(
                            &tx_clone_for_callback,
                            TuiEvent::AppendOutput(task_idx_for_callback, line),
                        );
                    });

                    let mut runner =
                        Runner::from_arc(Arc::clone(&config_arc)).with_output_callback(callback);
                    match runner.execute_fn_call(fn_name, &project_clone) {
                        Ok(_) => {
                            tui::send_event(
                                &tx_clone,
                                TuiEvent::UpdateStatus(task_idx, TaskStatus::Success),
                            );
                        }
                        Err(e) => {
                            tui::send_event(
                                &tx_clone,
                                TuiEvent::AppendOutput(task_idx, format!("Error: {}", e)),
                            );
                            tui::send_event(
                                &tx_clone,
                                TuiEvent::UpdateStatus(task_idx, TaskStatus::Error),
                            );
                            return Err(miette::miette!("{}", e));
                        }
                    }
                }
                Ok(())
            });

            if let Err(e) = app.run().await {
                return Err(miette::miette!("TUI error: {}", e));
            }

            handle
                .await
                .map_err(|e| miette::miette!("worker task failed: {}", e))??;
            Ok(())
        });
        result?;
    }
    Ok(())
}
