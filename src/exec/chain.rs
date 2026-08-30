use crate::exec::error::RuntimeError;
use crate::exec::{
    Executor, TaskOutcome, TaskStatus, TuiEvent, await_tasks_and_report, report_task_outcome,
};
use crate::ir::{Call, Ir};
use std::sync::Arc;
use tokio::sync::mpsc;

/// Execute a single chain of calls sequentially inside a blocking task.
///
/// Each call gets its own TUI task slot (computed from `start_index`), so the
/// chain's steps map directly onto contiguous rows in the model. Output lines
/// are forwarded through the TUI channel and each step's outcome is reported as
/// it completes; a failing step stops the chain and propagates the error so the
/// caller keeps the detail.
fn execute_single_chain(
    chain: Vec<Call>,
    start_index: usize,
    config: Arc<Ir>,
    tx: mpsc::UnboundedSender<TuiEvent>,
) -> Result<(), RuntimeError> {
    for (call_idx, call) in chain.iter().enumerate() {
        let task_idx = start_index + call_idx;
        let output_callback = {
            let tx = tx.clone();
            move |line: String| {
                crate::exec::send_tui_event(&tx, TuiEvent::AppendOutput(task_idx, line))
            }
        };
        let mut executor = Executor::new(config.clone(), Arc::new(output_callback));
        crate::exec::send_tui_event(&tx, TuiEvent::UpdateStatus(task_idx, TaskStatus::Running));

        let result = executor.execute_fn_call(&call.function, &call.project);
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

/// Execute run-block chains through the TUI.
///
/// A run block is an ordered list of chains. Calls joined by `=>` form one
/// sequential chain (each runs after the previous); `;` separates chains, which
/// run concurrently with one another. Every chain gets its own grouped header in
/// the TUI, with its calls listed underneath.
pub fn execute_task_chains(config: Arc<Ir>, chains: Vec<Vec<Call>>) -> Result<(), String> {
    // One TUI chain group per run-block chain, labelled by its joined calls.
    let chain_pairs: Vec<(String, Vec<String>)> = chains
        .iter()
        .map(|chain| {
            let task_names: Vec<String> = chain.iter().map(Call::fqn).collect();
            let label = task_names.join(" => ");
            (label, task_names)
        })
        .collect();

    crate::exec::run_tui_with_run(chain_pairs, move |tx| {
        let config = Arc::clone(&config);
        async move {
            let mut chain_handles = Vec::new();
            let mut base_index = 0;

            for chain in &chains {
                let tx = tx.clone();
                let config = Arc::clone(&config);
                let chain = chain.clone();
                let start_index = base_index;
                let chain_len = chain.len();

                let handle = tokio::task::spawn_blocking(move || {
                    execute_single_chain(chain, start_index, config, tx)
                });
                chain_handles.push((start_index + chain_len.saturating_sub(1), handle));
                base_index += chain_len;
            }

            await_tasks_and_report(&tx, chain_handles, "One or more chain tasks failed").await
        }
    })?;
    Ok(())
}
