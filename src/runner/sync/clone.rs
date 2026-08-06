use crate::plan::{PlanProject, SyncMode};
use crate::runner::colors;
use crate::runner::error::RuntimeError;
use crate::runner::{
    self, TaskOutcome, TaskStatus, TuiEvent, await_tasks_and_report, report_task_outcome,
};
use crate::shell;
use std::collections::BTreeMap;
use std::path::PathBuf;

/// Dispatch sync for a single project into `proj.dir` by its `SyncMode`.
/// The `match` is the only place that enumerates every strategy, so the
/// compiler forces a new variant to be handled here. Per-strategy
/// behavior lives in the co-located `run_sync_*` functions below.
fn sync_project_inner(
    proj_name: &str,
    proj: &PlanProject,
    output: &mut dyn FnMut(&str),
) -> Result<(), RuntimeError> {
    match proj.sync {
        SyncMode::Ignore => {
            output(&format!(
                "{} {} (sync=ignore)",
                colors::SYNC_PREFIXES[0],
                proj_name
            ));
            Ok(())
        }
        SyncMode::Clone => run_sync_clone_or_update(proj_name, proj, output),
    }
}

/// Git-clone a single project's repo into `proj.dir`, or fast-forward it to
/// its remote when the repo already exists. Progress is reported through the
/// `output` callback.
fn run_sync_clone_or_update(
    proj_name: &str,
    proj: &PlanProject,
    output: &mut dyn FnMut(&str),
) -> Result<(), RuntimeError> {
    let target_dir = PathBuf::from(&proj.dir);
    let target_dir_str = target_dir.to_string_lossy().to_string();

    if target_dir.join(".git").exists() {
        output(&format!(
            "{} {} → {}",
            colors::SYNC_PREFIXES[1],
            proj_name,
            target_dir.display()
        ));
        let args: Vec<&str> = match &proj.branch {
            Some(branch) => vec!["-C", &target_dir_str, "pull", "--ff-only", "origin", branch],
            None => vec!["-C", &target_dir_str, "pull", "--ff-only"],
        };
        return run_git_with_output("git pull", &args, proj_name, output);
    }

    output(&format!(
        "{} {} → {}",
        colors::SYNC_PREFIXES[2],
        proj_name,
        target_dir.display()
    ));
    let args: Vec<&str> = match &proj.branch {
        Some(branch) => vec!["clone", "-b", branch, &proj.url, &target_dir_str],
        None => vec!["clone", &proj.url, &target_dir_str],
    };
    run_git_with_output("git clone", &args, proj_name, output)
}

/// Spawn a `git` invocation through the shared subprocess runner, forward
/// its stdout/stderr lines through `output` (stdout indented like the
/// exec-statement output), and surface any failure as a `RuntimeError`
/// labelled with the built-once `full_cmd` description.
fn run_git_with_output(
    cmd_desc: &str,
    args: &[&str],
    proj_name: &str,
    output: &mut dyn FnMut(&str),
) -> Result<(), RuntimeError> {
    let full_cmd = format!("{} {}", cmd_desc, proj_name);
    let argv: Vec<&str> = std::iter::once("git").chain(args.iter().copied()).collect();
    let status =
        shell::run_subprocess(&full_cmd, &argv, None, None, None, &mut |line| match line {
            shell::SubprocessLine::Stdout(line) => output(&format!("    {}", line)),
            shell::SubprocessLine::Stderr(line) => output(&line),
        })
        .map_err(|e| RuntimeError::exec_io_error(&full_cmd, e))?;

    if !status.success() {
        return Err(RuntimeError::exec_io_error(
            &full_cmd,
            shell::describe_exit_failure(&status),
        ));
    }
    Ok(())
}

/// Synchronize a single project into `proj.dir`.
/// Accepts an output callback that receives progress lines for display or forwarding.
pub fn sync_project_with_callback(
    proj_name: &str,
    proj: &PlanProject,
    mut output_cb: impl FnMut(&str),
) -> Result<(), RuntimeError> {
    sync_project_inner(proj_name, proj, &mut output_cb)
}

/// Run sync for all projects through the TUI.
///
/// The sync chain list is derived from the project map itself: every project
/// is its own single-step chain labelled by its name, so the CLI cannot pass
/// a chain list that disagrees with the projects being synced. Each project
/// runs in its own blocking task that reports its own outcome, and
/// `await_tasks_and_report` reduces the results to a single aggregate error
/// (also surfacing any task panic).
pub fn run_sync_for_projects(mut projects: BTreeMap<String, PlanProject>) -> miette::Result<()> {
    let name_indices: Vec<(String, usize)> = projects
        .iter()
        .enumerate()
        .map(|(index, (name, _))| (name.clone(), index))
        .collect();
    let chain_pairs: Vec<(String, Vec<String>)> = name_indices
        .iter()
        .map(|(name, _)| (name.clone(), vec![name.clone()]))
        .collect();

    runner::run_tui_with_sync(chain_pairs, move |tx| async move {
        let mut task_handles = Vec::new();

        for (project_name, project_index) in name_indices {
            let Some(project) = projects.remove(&project_name) else {
                continue;
            };
            let tx_cb = tx.clone();
            let project_name_clone = project_name.clone();

            let handle = tokio::task::spawn_blocking(move || {
                runner::send_tui_event(
                    &tx_cb,
                    TuiEvent::UpdateStatus(project_index, TaskStatus::Running),
                );
                let result = crate::runner::sync::sync_project_with_callback(
                    &project_name_clone,
                    &project,
                    |line: &str| {
                        runner::send_tui_event(
                            &tx_cb,
                            TuiEvent::AppendOutput(project_index, line.to_string()),
                        );
                    },
                );
                report_task_outcome(
                    &tx_cb,
                    project_index,
                    match &result {
                        Ok(()) => TaskOutcome::Success,
                        Err(error) => TaskOutcome::Error(error),
                    },
                );
                result
            });

            task_handles.push((project_index, handle));
        }

        await_tasks_and_report(&tx, task_handles, "One or more projects failed to sync").await
    })?;
    Ok(())
}
