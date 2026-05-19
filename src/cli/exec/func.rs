use super::super::load_config_and_resolve;
use crate::runner::Runner;
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

    if !config.projects[&project].functions.contains_key(&name) {
        return Err(miette::miette!(
            "unknown function {} in project {}",
            name,
            project
        ));
    }

    if plain {
        let mut runner = Runner::new(config);
        runner
            .run_fn(&name, &project)
            .map_err(|e| miette::miette!("{}", e))?;
    } else {
        let rt = tokio::runtime::Runtime::new().map_err(|e| miette::miette!("{}", e))?;
        rt.block_on(async {
            let config = std::sync::Arc::new(config);

            let project_clone = project.clone();
            let name_clone = name.clone();

            let mut model = Model::new("fn".to_string(), format!("{}({})", name, project));
            model.add_task(format!("{}({})", name, project));

            let app = TuiApp::new(model);
            let tx = app.get_sender();

            let tx_clone = tx.clone();
            tokio::spawn(async move {
                tui::send_event(&tx_clone, TuiEvent::UpdateStatus(0, TaskStatus::Running));

                let callback: crate::runner::OutputCallback = Arc::new(move |line| {
                    tui::send_event(&tx_clone, TuiEvent::AppendOutput(0, line));
                });

                let mut runner =
                    crate::runner::Runner::from_arc(config).with_output_callback(callback);
                match runner.run_fn(&name_clone, &project_clone) {
                    Ok(_) => {
                        tui::send_event(&tx, TuiEvent::UpdateStatus(0, TaskStatus::Success));
                    }
                    Err(e) => {
                        tui::send_event(&tx, TuiEvent::AppendOutput(0, format!("Error: {}", e)));
                        tui::send_event(&tx, TuiEvent::UpdateStatus(0, TaskStatus::Error));
                    }
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
