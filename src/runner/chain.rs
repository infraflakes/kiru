use crate::compiler::Config;
use crate::runner::error::RuntimeError;
use crate::runner::{self, Runner, TaskOutcome, TaskStatus, TuiEvent, report_task_outcome};
use std::sync::Arc;
use std::sync::atomic::{AtomicUsize, Ordering};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

type ExecFn = Arc<dyn Fn(&mut Runner, &str) -> Result<(), RuntimeError> + Send + Sync>;

/// Execute a single chain of functions sequentially inside a blocking task.
/// Each function's output is forwarded through the TUI event channel.
fn execute_single_chain(
    chain: Vec<String>,
    start_index: usize,
    config: Arc<Config>,
    tx: mpsc::UnboundedSender<TuiEvent>,
    exec_fn: ExecFn,
) -> Result<(), ()> {
    let current_task = Arc::new(AtomicUsize::new(0));
    let output_callback = {
        let tx = tx.clone();
        let current_task = Arc::clone(&current_task);
        move |line: String| {
            let task_index = current_task.load(Ordering::Relaxed);
            runner::send_tui_event(&tx, TuiEvent::AppendOutput(task_index, line))
        }
    };
    let mut runner = Runner::new(config).with_output_callback(Arc::new(output_callback));

    for (fn_idx, function_name) in chain.iter().enumerate() {
        let task_idx = start_index + fn_idx;
        current_task.store(task_idx, Ordering::Relaxed);
        runner::send_tui_event(&tx, TuiEvent::UpdateStatus(task_idx, TaskStatus::Running));

        let outcome = match exec_fn(&mut runner, function_name) {
            Ok(()) => TaskOutcome::Success,
            Err(e) => TaskOutcome::Error(e),
        };
        if report_task_outcome(&tx, task_idx, outcome) {
            return Err(());
        }
    }

    Ok(())
}

/// Await all chain handles and collect whether any failed.
async fn collect_chain_results(
    chain_handles: Vec<JoinHandle<Result<(), ()>>>,
) -> Result<(), miette::Report> {
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

/// Execute a list of function chains through the TUI.
pub fn execute_task_chains(
    config: Arc<Config>,
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

                let handle = tokio::task::spawn_blocking(move || {
                    execute_single_chain(chain, start_index, config, tx, exec_fn)
                });

                chain_handles.push(handle);
                base_index += chain_len;
            }

            collect_chain_results(chain_handles).await
        }
    })?;
    Ok(())
}
