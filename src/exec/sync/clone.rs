use crate::exec::colors;
use crate::exec::error::RuntimeError;
use crate::exec::subprocess;
use crate::exec::{TaskOutcome, TaskStatus, TuiEvent, await_tasks_and_report, report_task_outcome};
use std::path::PathBuf;

/// A plain repo configuration read from `kiru.toml`, used by sync.
#[derive(Debug, Clone)]
pub(crate) struct RepoSync {
    pub(crate) name: String,
    pub(crate) url: String,
    pub(crate) dir: String,
    pub(crate) branch: String,
}

/// Dispatch sync for a single project into `repo.dir` by cloning or pulling.
fn sync_project_inner(repo: &RepoSync, output: &mut dyn FnMut(&str)) -> Result<(), RuntimeError> {
    run_sync_clone_or_update(repo, output)
}

/// Git-clone a single project's repo into `repo.dir`, or fast-forward it to its
/// remote when the repo already exists. Progress is reported through `output`.
fn run_sync_clone_or_update(
    repo: &RepoSync,
    output: &mut dyn FnMut(&str),
) -> Result<(), RuntimeError> {
    let target_dir = PathBuf::from(&repo.dir);
    let target_dir_str = target_dir.to_string_lossy().to_string();

    if target_dir.join(".git").exists() {
        output(&format!(
            "{} {} -> {}",
            colors::SYNC_PREFIXES[1],
            repo.name,
            target_dir.display()
        ));
        let args: Vec<&str> = if repo.branch.is_empty() {
            vec!["-C", &target_dir_str, "pull", "--ff-only"]
        } else {
            vec![
                "-C",
                &target_dir_str,
                "pull",
                "--ff-only",
                "origin",
                &repo.branch,
            ]
        };
        return run_git_with_output("git pull", &args, &repo.name, output);
    }

    output(&format!(
        "{} {} -> {}",
        colors::SYNC_PREFIXES[2],
        repo.name,
        target_dir.display()
    ));
    let args: Vec<&str> = if repo.branch.is_empty() {
        vec!["clone", &repo.url, &target_dir_str]
    } else {
        vec!["clone", "-b", &repo.branch, &repo.url, &target_dir_str]
    };
    run_git_with_output("git clone", &args, &repo.name, output)
}

/// Spawn a `git` invocation through the shared subprocess runner, forward its
/// stdout/stderr lines through `output`, and surface any failure as a
/// `RuntimeError` labelled with the built-once `full_cmd` description.
fn run_git_with_output(
    cmd_desc: &str,
    args: &[&str],
    proj_name: &str,
    output: &mut dyn FnMut(&str),
) -> Result<(), RuntimeError> {
    let full_cmd = format!("{} {}", cmd_desc, proj_name);
    let argv: Vec<&str> = std::iter::once("git").chain(args.iter().copied()).collect();
    let status =
        subprocess::run_subprocess(&full_cmd, &argv, None, None, None, &mut |line| match line {
            subprocess::SubprocessLine::Stdout(line) => output(&format!("    {}", line)),
            subprocess::SubprocessLine::Stderr(line) => output(&line),
        })
        .map_err(|e| RuntimeError::exec_io_error(&full_cmd, e))?;

    if !status.success() {
        return Err(RuntimeError::exec_io_error(
            &full_cmd,
            subprocess::describe_exit_failure(&status),
        ));
    }
    Ok(())
}

/// Synchronize a single project into `repo.dir`.
/// Accepts an output callback that receives progress lines for display or forwarding.
pub(crate) fn sync_project_with_callback(
    repo: &RepoSync,
    mut output_cb: impl FnMut(&str),
) -> Result<(), RuntimeError> {
    sync_project_inner(repo, &mut output_cb)
}

/// Run sync for all projects through the TUI.
///
/// The sync chain list is derived from the repo list itself: every project is
/// its own single-step chain labelled by its name, so the CLI cannot pass a
/// chain list that disagrees with the projects being synced. Each project runs
/// in its own blocking task that reports its own outcome, and
/// `await_tasks_and_report` reduces the results to a single aggregate error
/// (also surfacing any task panic).
pub(crate) fn run_sync_for_projects(repos: Vec<RepoSync>) -> Result<(), String> {
    let name_indices: Vec<(String, usize)> = repos
        .iter()
        .enumerate()
        .map(|(index, repo)| (repo.name.clone(), index))
        .collect();
    let chain_pairs: Vec<(String, Vec<String>)> = name_indices
        .iter()
        .map(|(name, _)| (name.clone(), vec![name.clone()]))
        .collect();

    crate::exec::run_tui_with_sync(chain_pairs, move |tx| async move {
        let mut task_handles = Vec::new();

        for (project_name, project_index) in name_indices {
            let repo = match repos.iter().find(|r| r.name == project_name) {
                Some(r) => r.clone(),
                None => continue,
            };
            let tx_cb = tx.clone();

            let handle = tokio::task::spawn_blocking(move || {
                crate::exec::send_tui_event(
                    &tx_cb,
                    TuiEvent::UpdateStatus(project_index, TaskStatus::Running),
                );
                let result = crate::exec::sync::sync_project_with_callback(&repo, |line: &str| {
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

        await_tasks_and_report(&tx, task_handles, "One or more projects failed to sync").await
    })?;
    Ok(())
}
