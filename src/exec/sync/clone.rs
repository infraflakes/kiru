use crate::exec::colors;
use crate::exec::error::RuntimeError;
use crate::exec::subprocess;
use crate::exec::{TaskOutcome, TaskStatus, TuiEvent, await_tasks_and_report, report_task_outcome};
use crate::ir::{Segment, Sync, Template};
use std::collections::BTreeMap;
use std::path::PathBuf;

/// Resolve a sync `Template` to a concrete string at runtime: literals are
/// concatenated verbatim and `$(command)` parts are run through `shell -c`,
/// with their stdout spliced in. Used for sync `url`/`dir`/`branch`/`strategy`,
/// which may contain runtime commands.
fn resolve_sync_value(tmpl: &Template, shell: &str) -> String {
    let mut out = String::new();
    for segment in &tmpl.segments {
        match segment {
            Segment::Literal(s) => out.push_str(s),
            Segment::Command(inner) => {
                let cmd = resolve_sync_value(inner, shell);
                out.push_str(&run_sync_capture(&cmd, shell));
            }
        }
    }
    out
}

/// Run `cmd` via `shell -c` and return its trimmed stdout (non-fatal on error).
fn run_sync_capture(cmd: &str, shell: &str) -> String {
    let mut captured = String::new();
    let _ = subprocess::run_subprocess(cmd, &[shell, "-c", cmd], None, None, None, &mut |line| {
        match line {
            subprocess::SubprocessLine::Stdout(text) => captured.push_str(&text),
            subprocess::SubprocessLine::Stderr(_) => {}
        }
    });
    captured.trim_end().to_string()
}

/// Dispatch sync for a single project into `sync.dir` by its strategy. A
/// `sync = ignore` strategy is a no-op; anything else clones/pulls the repo.
/// The `if` is the only place that branches on the strategy, so a new strategy
/// is handled here. Per-strategy behavior lives in `run_sync_clone_or_update`.
fn sync_project_inner(
    proj_name: &str,
    sync: &Sync,
    output: &mut dyn FnMut(&str),
    shell: &str,
) -> Result<(), RuntimeError> {
    let strategy = resolve_sync_value(&sync.strategy, shell);
    if strategy.trim().eq_ignore_ascii_case("ignore") {
        output(&format!(
            "{} {} (sync=ignore)",
            colors::SYNC_PREFIXES[0],
            proj_name
        ));
        return Ok(());
    }
    run_sync_clone_or_update(proj_name, sync, output, shell)
}

/// Git-clone a single project's repo into `sync.dir`, or fast-forward it to its
/// remote when the repo already exists. Progress is reported through `output`.
fn run_sync_clone_or_update(
    proj_name: &str,
    sync: &Sync,
    output: &mut dyn FnMut(&str),
    shell: &str,
) -> Result<(), RuntimeError> {
    let url = resolve_sync_value(&sync.url, shell);
    let dir = resolve_sync_value(&sync.dir, shell);
    let branch = resolve_sync_value(&sync.branch, shell);
    let target_dir = PathBuf::from(&dir);
    let target_dir_str = target_dir.to_string_lossy().to_string();

    if target_dir.join(".git").exists() {
        output(&format!(
            "{} {} → {}",
            colors::SYNC_PREFIXES[1],
            proj_name,
            target_dir.display()
        ));
        let args: Vec<&str> = if branch.is_empty() {
            vec!["-C", &target_dir_str, "pull", "--ff-only"]
        } else {
            vec![
                "-C",
                &target_dir_str,
                "pull",
                "--ff-only",
                "origin",
                &branch,
            ]
        };
        return run_git_with_output("git pull", &args, proj_name, output);
    }

    output(&format!(
        "{} {} → {}",
        colors::SYNC_PREFIXES[2],
        proj_name,
        target_dir.display()
    ));
    let args: Vec<&str> = if branch.is_empty() {
        vec!["clone", &url, &target_dir_str]
    } else {
        vec!["clone", "-b", &branch, &url, &target_dir_str]
    };
    run_git_with_output("git clone", &args, proj_name, output)
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

/// Synchronize a single project into `sync.dir`.
/// Accepts an output callback that receives progress lines for display or forwarding.
pub fn sync_project_with_callback(
    proj_name: &str,
    sync: &Sync,
    mut output_cb: impl FnMut(&str),
    shell: &str,
) -> Result<(), RuntimeError> {
    sync_project_inner(proj_name, sync, &mut output_cb, shell)
}

/// Run sync for all projects through the TUI.
///
/// The sync chain list is derived from the sync map itself: every project is
/// its own single-step chain labelled by its name, so the CLI cannot pass a
/// chain list that disagrees with the projects being synced. Each project runs
/// in its own blocking task that reports its own outcome, and
/// `await_tasks_and_report` reduces the results to a single aggregate error
/// (also surfacing any task panic).
pub fn run_sync_for_projects(syncs: BTreeMap<String, Sync>, shell: &str) -> Result<(), String> {
    let shell = shell.to_string();
    let name_indices: Vec<(String, usize)> = syncs
        .iter()
        .enumerate()
        .map(|(index, (name, _))| (name.clone(), index))
        .collect();
    let chain_pairs: Vec<(String, Vec<String>)> = name_indices
        .iter()
        .map(|(name, _)| (name.clone(), vec![name.clone()]))
        .collect();

    crate::exec::run_tui_with_sync(chain_pairs, move |tx| {
        let syncs = syncs;
        let shell = shell;
        async move {
            let mut task_handles = Vec::new();

            for (project_name, project_index) in name_indices {
                let sync = match syncs.get(&project_name) {
                    Some(s) => s.clone(),
                    None => continue,
                };
                let tx_cb = tx.clone();
                let project_name_clone = project_name.clone();
                let shell = shell.clone();

                let handle = tokio::task::spawn_blocking(move || {
                    crate::exec::send_tui_event(
                        &tx_cb,
                        TuiEvent::UpdateStatus(project_index, TaskStatus::Running),
                    );
                    let result = crate::exec::sync::sync_project_with_callback(
                        &project_name_clone,
                        &sync,
                        |line: &str| {
                            crate::exec::send_tui_event(
                                &tx_cb,
                                TuiEvent::AppendOutput(project_index, line.to_string()),
                            );
                        },
                        &shell,
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
        }
    })?;
    Ok(())
}
