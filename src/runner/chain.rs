use crate::plan::Plan;
use crate::plan::QualifiedFnRef;
use crate::runner::error::RuntimeError;
use crate::runner::{
    self, Runner, TaskOutcome, TaskStatus, TuiEvent, await_tasks_and_report, report_task_outcome,
};
use std::sync::Arc;
use tokio::sync::mpsc;

/// Execute a single chain of functions sequentially inside a blocking task.
///
/// Each step gets its own `Runner` whose output callback captures the step's
/// task slot directly, so no shared current-task index is needed. Output lines
/// are forwarded through the TUI event channel and each step's outcome is
/// reported as it completes. The chain's failing error is propagated so the
/// caller keeps the detail.
fn execute_single_chain(
    chain: Vec<QualifiedFnRef>,
    start_index: usize,
    config: Arc<Plan>,
    tx: mpsc::UnboundedSender<TuiEvent>,
) -> Result<(), RuntimeError> {
    for (fn_idx, qualified) in chain.iter().enumerate() {
        let task_idx = start_index + fn_idx;
        let output_callback = {
            let tx = tx.clone();
            move |line: String| runner::send_tui_event(&tx, TuiEvent::AppendOutput(task_idx, line))
        };
        let mut runner = Runner::new(config.clone(), Arc::new(output_callback));
        runner::send_tui_event(&tx, TuiEvent::UpdateStatus(task_idx, TaskStatus::Running));

        let result = runner.execute_fn_call(&qualified.function, &qualified.project);
        report_task_outcome(
            &tx,
            task_idx,
            match &result {
                Ok(()) => TaskOutcome::Success,
                Err(error) => TaskOutcome::Error(error),
            },
        );
        result?;
    }

    Ok(())
}

/// Execute a list of function chains through the TUI.
pub fn execute_task_chains(
    config: Arc<Plan>,
    chains: Vec<Vec<QualifiedFnRef>>,
) -> miette::Result<()> {
    let (chain_pairs, chain_tasks): (Vec<_>, Vec<_>) = chains
        .iter()
        .map(|chain| {
            let task_names: Vec<String> = chain.iter().map(QualifiedFnRef::fqn).collect();
            let label = task_names.join(" → ");
            ((label, task_names), chain.clone())
        })
        .unzip();

    runner::run_tui_with_run(chain_pairs, move |tx| {
        let config = Arc::clone(&config);
        async move {
            let mut chain_handles = Vec::new();
            let mut base_index = 0;

            for chain in &chain_tasks {
                let tx = tx.clone();
                let config = Arc::clone(&config);
                let chain = chain.clone();
                let start_index = base_index;
                let chain_len = chain.len();

                let handle = tokio::task::spawn_blocking(move || {
                    execute_single_chain(chain, start_index, config, tx)
                });

                // The chain's final step is the anchor used to surface a
                // panic: any step that already reported keeps its outcome,
                // and the last slot is the one most likely still pending.
                let panic_anchor = start_index + chain_len.saturating_sub(1);
                chain_handles.push((panic_anchor, handle));
                base_index += chain_len;
            }

            await_tasks_and_report(&tx, chain_handles, "One or more chain tasks failed").await
        }
    })?;
    Ok(())
}
