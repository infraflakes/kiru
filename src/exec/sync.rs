//! Repository sync: clones or fast-forward-pulls declared repositories
//! into their configured directories, reporting each project through the
//! shared TUI shell.
//!
//! Sync is an all-settle batch: every project runs concurrently in its own
//! scoped thread and a failure never cancels its siblings. It reuses the
//! same display state and TUI shell as `run`; only the scheduling policy
//! differs.

use crate::exec::colors;
use crate::exec::error::RuntimeError;
use crate::exec::model::{Display, TaskStatus};
use crate::exec::subprocess::{RunKillSwitch, run_subprocess};
use crate::exec::{TaskRunError, render_sync_output};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

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
/// Every project is one display row; all rows run concurrently in scoped
/// threads and are joined before the sync finishes. A failure is reported
/// on its own row and never cancels the others (all-settle), but a keyboard
/// cancel kills every running git process.
pub(crate) fn run_sync_for_projects(projects: Vec<ProjectSync>) -> Result<(), TaskRunError> {
    let labels: Vec<String> = projects.iter().map(|p| p.name.clone()).collect();
    let row_count = labels.len();
    let display = Arc::new(Mutex::new(Display::with_labels(labels)));
    let kill = Arc::new(RunKillSwitch::new());
    let kill_for_cancel = Arc::clone(&kill);

    let display_for_worker = Arc::clone(&display);
    let worker = move || {
        std::thread::scope(|scope| {
            let mut handles = Vec::new();
            for (index, project) in projects.into_iter().enumerate() {
                let project_display = Arc::clone(&display_for_worker);
                let project_kill = Arc::clone(&kill);
                handles.push((
                    index,
                    scope.spawn(move || {
                        project_display
                            .lock()
                            .unwrap_or_else(|e| e.into_inner())
                            .set_status(index, TaskStatus::Running);
                        let result = run_sync_clone_or_update(&project, &project_kill, |line| {
                            project_display
                                .lock()
                                .unwrap_or_else(|e| e.into_inner())
                                .push_output(index, line.to_string());
                        });
                        let status = match &result {
                            Ok(()) => TaskStatus::Success,
                            Err(RuntimeError::Cancelled(_)) => TaskStatus::Cancelled,
                            Err(_) => TaskStatus::Error,
                        };
                        project_display
                            .lock()
                            .unwrap_or_else(|e| e.into_inner())
                            .set_status(index, status);
                        result
                    }),
                ));
            }

            let mut failed = false;
            for (index, handle) in handles {
                match handle.join() {
                    Ok(Ok(())) => {}
                    Ok(Err(_)) => failed = true,
                    Err(_) => {
                        // A panicked project thread is a genuine defect.
                        let mut guard =
                            display_for_worker.lock().unwrap_or_else(|e| e.into_inner());
                        guard.set_status(index, TaskStatus::Error);
                        guard.push_output(index, "Task panicked".to_string());
                        failed = true;
                    }
                }
            }
            if failed { Err(()) } else { Ok(()) }
        })
    };

    // Sync has no final text dump: the per-project rows are the report.
    match crate::exec::tui::run_tui(
        display,
        row_count,
        worker,
        render_sync_output,
        None::<fn(&Display) -> String>,
        Some(kill_for_cancel),
    ) {
        Ok(Ok(())) => Ok(()),
        Ok(Err(())) => Err(TaskRunError::TaskFailed),
        Err(message) => Err(TaskRunError::Infrastructure(message)),
    }
}
