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
        let config = Arc::new(config);

        let task_names: Vec<String> = fns
            .iter()
            .map(|fn_name| format!("{}({})", fn_name, project))
            .collect();

        tui::run_tui_with(task_names, move |tx| {
            let project = project.clone();
            async move {
                for (task_idx, fn_name) in fns.iter().enumerate() {
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
                            return Err(miette::miette!("{}", e));
                        }
                    }
                }
                Ok(())
            }
        })?;
    }
    Ok(())
}
