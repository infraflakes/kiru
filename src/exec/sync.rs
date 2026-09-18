//! Repository sync: clones or fast-forward-pulls declared repositories
//! into their configured directories, reporting each project through the
//! sync TUI.

use crate::exec::colors;
use crate::exec::error::RuntimeError;
use crate::exec::subprocess::{RunKillSwitch, run_subprocess};
use crate::exec::{
    TaskOutcome, TaskRunError, TaskStatus, TuiEvent, await_tasks_and_report, render_sync_output,
    report_task_outcome,
};
use std::path::PathBuf;
use std::sync::Arc;

/// A plain project configuration read from `kiru.toml`, used by sync.
#[derive(Debug, Clone)]
pub(crate) struct ProjectSync {
    pub(crate) name: String,
    pub(crate) url: String,
    pub(crate) dir: String,
    pub(crate) branch: String,
}

/// Git-clone a single project's repo into `project.dir`, or fast-forward it to its
/// remote when the repo already exists. Progress lines go to `output` for
/// display or forwarding. `kill` registers the git process group so a keyboard
/// cancel can stop the clone.
fn run_sync_clone_or_update(
    project: &ProjectSync,
    kill: &RunKillSwitch,
    mut output: impl FnMut(&str),
) -> Result<(), RuntimeError> {
    let target_dir = PathBuf::from(&project.dir);
    let target_dir_str = target_dir.to_string_lossy().to_string();

    if target_dir.join(".git").exists() {
        output(&format!(
            "{} {} -> {}",
            colors::SYNC_UPDATE_PREFIX,
            project.name,
            target_dir.display()
        ));
        let args: Vec<&str> = if project.branch.is_empty() {
            vec!["-C", &target_dir_str, "pull", "--ff-only"]
        } else {
            vec![
                "-C",
                &target_dir_str,
                "pull",
                "--ff-only",
                "origin",
                &project.branch,
            ]
        };
        return run_git_with_output("git pull", &args, &project.name, kill, &mut output);
    }

    output(&format!(
        "{} {} -> {}",
        colors::SYNC_CLONE_PREFIX,
        project.name,
        target_dir.display()
    ));
    let args: Vec<&str> = if project.branch.is_empty() {
        vec!["clone", &project.url, &target_dir_str]
    } else {
        vec![
            "clone",
            "-b",
            &project.branch,
            &project.url,
            &target_dir_str,
        ]
    };
    run_git_with_output("git clone", &args, &project.name, kill, &mut output)
}

/// Spawn a `git` invocation through the shared subprocess runner, forward its
/// stdout/stderr lines through `output`, and surface any failure as a
/// `RuntimeError` labelled with the built-once `full_cmd` description.
fn run_git_with_output(
    cmd_desc: &str,
    args: &[&str],
    proj_name: &str,
    kill: &RunKillSwitch,
    output: &mut dyn FnMut(&str),
) -> Result<(), RuntimeError> {
    let full_cmd = format!("{} {}", cmd_desc, proj_name);
    let argv: Vec<&str> = std::iter::once("git").chain(args.iter().copied()).collect();
    let exit = run_subprocess(
        &full_cmd,
        &argv,
        None,
        None,
        None,
        Some(kill),
        &mut |line| match line {
            crate::exec::subprocess::SubprocessLine::Stdout(line) => {
                output(&format!("    {}", line))
            }
            crate::exec::subprocess::SubprocessLine::Stderr(line) => output(&line),
        },
    )
    .map_err(|e| RuntimeError::exec_io_error(&full_cmd, e))?;

    if !exit.status.success() {
        return Err(RuntimeError::exec_io_error(
            &full_cmd,
            crate::exec::subprocess::describe_exit_failure(&exit.status),
        ));
    }
    Ok(())
}

/// Run sync for all projects through the TUI.
///
/// The sync chain list is derived from the project list itself: every project is
/// its own single-step chain labelled by its name, so the CLI cannot pass a
/// chain list that disagrees with the projects being synced. Each project runs
/// in its own blocking task that reports its own outcome, and
/// `await_tasks_and_report` reduces the results to a single outcome
/// (also surfacing any task panic).
pub(crate) fn run_sync_for_projects(projects: Vec<ProjectSync>) -> Result<(), TaskRunError> {
    // One display line per project: sync has no steps, just projects.
    let plan: Vec<crate::ir::PlanLine> = projects
        .iter()
        .enumerate()
        .map(|(row, project)| crate::ir::PlanLine {
            row,
            depth: 0,
            label: project.name.clone(),
            project: None,
            // Sync draws its own per-project list; the tree prefixes are
            // only used by the run views.
            prefix: String::new(),
            output_prefix: String::new(),
        })
        .collect();
    // Sync failures are all-settle (a failed clone does not abort other
    // clones), but a keyboard cancel kills every running git process.
    let kill = Arc::new(RunKillSwitch::new());
    // Cloned before the worker closure moves `kill` into the async task:
    // the cancel path needs the same kill switch.
    let kill_for_cancel = Arc::clone(&kill);

    match crate::exec::run_tui_with(
        plan,
        move |tx| async move {
            let mut task_handles = Vec::new();

            for (project_index, project) in projects.into_iter().enumerate() {
                let tx_cb = tx.clone();
                let kill = Arc::clone(&kill);

                let handle = tokio::task::spawn_blocking(move || {
                    crate::exec::send_tui_event(
                        &tx_cb,
                        TuiEvent::UpdateStatus(project_index, TaskStatus::Running),
                    );
                    let result = run_sync_clone_or_update(&project, &kill, |line: &str| {
                        crate::exec::send_tui_event(
                            &tx_cb,
                            TuiEvent::AppendOutput(project_index, line.to_string()),
                        );
                    });
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

            await_tasks_and_report(&tx, task_handles).await
        },
        render_sync_output,
        None,
        Some(kill_for_cancel),
    ) {
        Ok(worker_result) => worker_result,
        Err(message) => Err(TaskRunError::Infrastructure(message)),
    }
}
