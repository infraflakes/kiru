//! Chain execution: runs a sequential chain of project-function calls
//! through the TUI, reporting each step's status as it completes.

use crate::exec::error::RuntimeError;
use crate::exec::tui::run::{format_final_output, render_run_output};
use crate::exec::{
    Executor, TaskOutcome, TaskRunError, TaskStatus, TuiEvent, await_tasks_and_report,
    report_task_outcome,
};
use crate::ir::{Call, Ir};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::mpsc;

/// Shared configuration passed through chain execution.
struct ChainConfig {
    ir: Arc<Ir>,
    shell: String,
    timeout: Option<std::time::Duration>,
    repo_dirs: BTreeMap<String, PathBuf>,
    invocation_cwd: PathBuf,
}

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
    config: &ChainConfig,
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
        let mut executor = Executor::new(
            config.ir.clone(),
            config.shell.clone(),
            config.timeout,
            Arc::new(output_callback),
        );
        crate::exec::send_tui_event(&tx, TuiEvent::UpdateStatus(task_idx, TaskStatus::Running));

        let cwd = config
            .repo_dirs
            .get(&call.project)
            .cloned()
            .unwrap_or_else(|| config.invocation_cwd.clone());

        let result = executor.execute_fn_call(&call.function, &call.project, cwd);
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
pub(crate) fn execute_task_chains(
    ir: Arc<Ir>,
    chains: Vec<Vec<Call>>,
    shell: String,
    timeout: Option<std::time::Duration>,
    repo_dirs: BTreeMap<String, PathBuf>,
    invocation_cwd: PathBuf,
) -> Result<(), TaskRunError> {
    // One TUI chain group per run-block chain, labelled by its joined calls.
    let chain_pairs: Vec<(String, Vec<String>)> = chains
        .iter()
        .map(|chain| {
            let task_names: Vec<String> = chain.iter().map(Call::fqn).collect();
            let label = task_names.join(" => ");
            (label, task_names)
        })
        .collect();

    let chain_config = Arc::new(ChainConfig {
        ir,
        shell,
        timeout,
        repo_dirs,
        invocation_cwd,
    });

    match crate::exec::run_tui_with(
        chain_pairs,
        move |tx| async move {
            let mut chain_handles = Vec::new();
            let mut base_index = 0;

            for chain in &chains {
                let tx = tx.clone();
                let chain = chain.clone();
                let start_index = base_index;
                let chain_len = chain.len();
                let config = Arc::clone(&chain_config);

                let handle = tokio::task::spawn_blocking(move || {
                    execute_single_chain(chain, start_index, &config, tx)
                });
                chain_handles.push((start_index + chain_len.saturating_sub(1), handle));
                base_index += chain_len;
            }

            await_tasks_and_report(&tx, chain_handles).await
        },
        render_run_output,
        Some(format_final_output),
    ) {
        Ok(worker_result) => worker_result,
        Err(message) => Err(TaskRunError::Infrastructure(message)),
    }
}
