//! Chain execution: runs a sequential chain of project-function calls
//! through the TUI, reporting each step's status as it completes.

use crate::exec::error::RuntimeError;
use crate::exec::executor::ProjectExec;
use crate::exec::subprocess::RunKillSwitch;
use crate::exec::{
    Executor, TaskOutcome, TaskRunError, TaskStatus, TuiEvent, await_tasks_and_report,
    format_final_output, render_run_output, report_task_outcome,
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
    /// Working directory and direnv setup per project; the executor
    /// resolves the project's starting directory from this, falling back
    /// to the invocation cwd.
    projects: Arc<BTreeMap<String, ProjectExec>>,
    invocation_cwd: PathBuf,
    /// Run-level kill switch: one failing chain stops the whole run.
    kill: Arc<RunKillSwitch>,
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
        // Fail-fast: once any chain of the run has failed, no new command
        // starts. This step and every later one in the chain never run;
        // mark them cancelled so the chain ends terminal and its header
        // matches its rows.
        if config.kill.is_failed() {
            for pending_idx in call_idx..chain.len() {
                crate::exec::send_tui_event(
                    &tx,
                    TuiEvent::UpdateStatus(start_index + pending_idx, TaskStatus::Cancelled),
                );
            }
            return Err(RuntimeError::Cancelled(
                "run failed in another chain".to_string(),
            ));
        }
        let task_idx = start_index + call_idx;
        let output_callback = {
            let tx = tx.clone();
            move |line: String| {
                crate::exec::send_tui_event(&tx, TuiEvent::AppendOutput(task_idx, line))
            }
        };
        let mut executor = Executor::new(
            config.ir.clone(),
            Arc::clone(&config.projects),
            config.invocation_cwd.clone(),
            config.shell.clone(),
            config.timeout,
            Arc::new(output_callback),
            Some(Arc::clone(&config.kill)),
        );
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
        if let Err(error) = result {
            // Fail-fast: the whole run stops; running sibling chains get
            // their process groups killed so nothing keeps running. The
            // remaining steps of this chain never run; mark them cancelled
            // so the chain display is terminal and unified.
            for later_idx in (call_idx + 1)..chain.len() {
                crate::exec::send_tui_event(
                    &tx,
                    TuiEvent::UpdateStatus(start_index + later_idx, TaskStatus::Cancelled),
                );
            }
            config.kill.fail();
            return Err(error);
        }
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
    projects: Arc<BTreeMap<String, ProjectExec>>,
    invocation_cwd: PathBuf,
    shell: String,
    timeout: Option<std::time::Duration>,
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
        projects,
        invocation_cwd,
        kill: Arc::new(RunKillSwitch::new()),
    });
    // Cloned before the worker closure moves `chain_config` into the async
    // task: the cancel path needs the same kill switch.
    let kill_for_cancel = Arc::clone(&chain_config.kill);

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
        Some(kill_for_cancel),
    ) {
        Ok(worker_result) => worker_result,
        Err(message) => Err(TaskRunError::Infrastructure(message)),
    }
}
