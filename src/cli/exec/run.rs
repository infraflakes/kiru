use super::super::load_config;
use crate::runner::Runner;
use crate::runner::error::RuntimeError;
use crate::runner::{self, TaskStatus, TuiEvent};
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};

/// Execute a list of function chains through the TUI, calling `exec_fn` for
/// each step.  `task_name_fn` produces the label shown in the UI for each
/// function call (e.g. `"fn_name(project)"` for project-scoped runs).
fn run_chains(
    config: Arc<crate::compiler::Sanctuary>,
    chains: Vec<Vec<String>>,
    task_name_fn: impl Fn(&str) -> String + Send + 'static,
    exec_fn: impl Fn(&mut Runner, &str) -> Result<(), RuntimeError> + Send + Sync + 'static,
) -> miette::Result<()> {
    let (chain_pairs, chain_tasks): (Vec<_>, Vec<_>) = chains
        .iter()
        .map(|chain| {
            let label = chain.join(" → ");
            let task_names: Vec<String> =
                chain.iter().map(|fn_name| task_name_fn(fn_name)).collect();
            ((label, task_names), chain.clone())
        })
        .unzip();

    let exec_fn = Arc::new(exec_fn);
    runner::run_tui_with_run(chain_pairs, move |tx| {
        let config = Arc::clone(&config);
        let exec_fn = Arc::clone(&exec_fn);
        async move {
            let mut chain_handles = Vec::new();

            let mut base_index = 0;
            for chain in &chain_tasks {
                let tx = tx.clone();
                let config = Arc::clone(&config);
                let exec_fn = Arc::clone(&exec_fn);
                let chain = chain.clone();
                let start_index = base_index;
                let chain_len = chain.len();

                let handle = tokio::task::spawn_blocking(move || -> Result<(), ()> {
                    let current_task = Arc::new(AtomicUsize::new(0));
                    let output_callback = {
                        let tx = tx.clone();
                        let current_task = Arc::clone(&current_task);
                        move |line: String| {
                            let task_index = current_task.load(Ordering::Relaxed);
                            runner::send_tui_event(&tx, TuiEvent::AppendOutput(task_index, line))
                        }
                    };
                    let mut runner = Runner::new(Arc::clone(&config))
                        .with_output_callback(Arc::new(output_callback));

                    for (fn_idx, function_name) in chain.iter().enumerate() {
                        let task_idx = start_index + fn_idx;
                        current_task.store(task_idx, Ordering::Relaxed);
                        runner::send_tui_event(
                            &tx,
                            TuiEvent::UpdateStatus(task_idx, TaskStatus::Running),
                        );

                        match exec_fn(&mut runner, function_name) {
                            Ok(()) => {
                                runner::send_tui_event(
                                    &tx,
                                    TuiEvent::UpdateStatus(task_idx, TaskStatus::Success),
                                );
                            }
                            Err(e) => {
                                runner::send_tui_event(
                                    &tx,
                                    TuiEvent::AppendOutput(task_idx, format!("Error: {}", e)),
                                );
                                runner::send_tui_event(
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

/// Execute chains scoped to a named project.
fn run_project_chains(
    config: Arc<crate::compiler::Sanctuary>,
    project: &str,
    chains: Vec<Vec<String>>,
) -> miette::Result<()> {
    let project = project.to_string();
    run_chains(
        config,
        chains,
        {
            let project = project.clone();
            move |function_name| format!("{}({})", function_name, project)
        },
        move |runner, function_name| runner.execute_fn_call(function_name, &project),
    )
}

pub fn execute_run_block(
    config_arg: Option<PathBuf>,
    name: String,
    project: Option<String>,
) -> miette::Result<()> {
    let config = load_config(config_arg)?;

    match project {
        Some(ref project_name) => {
            if !config.projects.contains_key(project_name) {
                return Err(miette::miette!("unknown project: {}", project_name));
            }

            let project_entry = &config.projects[project_name];
            let chains: Vec<Vec<String>> = match project_entry.runs.get(&name) {
                Some(chain_list) => chain_list.clone(),
                None => {
                    return Err(miette::miette!(
                        "unknown run block '{}' in project '{}'",
                        name,
                        project_name
                    ));
                }
            };

            run_project_chains(Arc::new(config), project_name, chains)
        }
        None => Err(miette::miette!(
            "must specify a project to run '{}' in",
            name
        )),
    }
}
