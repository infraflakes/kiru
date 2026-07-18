use crate::plan::{PlanProject, SyncMode};
use crate::runner::error::RuntimeError;
use crate::runner::{self, TaskOutcome, TaskStatus, TuiEvent, report_task_outcome};
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Command;
use std::sync::mpsc;
use std::thread;

type GitOutputReaders = (
    mpsc::Receiver<String>,
    Option<thread::JoinHandle<()>>,
    Option<thread::JoinHandle<()>>,
    thread::JoinHandle<std::io::Result<std::process::ExitStatus>>,
);

/// Spawn reader threads for git command stdout/stderr, forwarding each line
/// through an mpsc channel. Returns the channel receiver, reader thread handles,
/// and a wait handle for the child process.
fn spawn_git_output_readers(mut child: std::process::Child) -> GitOutputReaders {
    use std::io::BufRead;

    let (line_sender, line_receiver) = mpsc::channel::<String>();

    let stdout_handle = child.stdout.take().map(|stream| {
        let sender = line_sender.clone();
        thread::spawn(move || {
            for line in std::io::BufReader::new(stream)
                .lines()
                .map_while(Result::ok)
            {
                let _ = sender.send(format!("    {}", line));
            }
        })
    });
    let stderr_handle = child.stderr.take().map(|stream| {
        let sender = line_sender.clone();
        thread::spawn(move || {
            for line in std::io::BufReader::new(stream)
                .lines()
                .map_while(Result::ok)
            {
                let _ = sender.send(line);
            }
        })
    });
    drop(line_sender);

    let wait_handle = thread::spawn(move || child.wait());

    (line_receiver, stdout_handle, stderr_handle, wait_handle)
}

/// Drain all lines from the channel (feeding them to `output`), wait for the
/// git process, and validate its exit status.
fn drain_and_check_git_output(
    proj_name: &str,
    line_receiver: mpsc::Receiver<String>,
    stdout_handle: Option<thread::JoinHandle<()>>,
    stderr_handle: Option<thread::JoinHandle<()>>,
    wait_handle: thread::JoinHandle<std::io::Result<std::process::ExitStatus>>,
    output: &mut dyn FnMut(&str),
) -> Result<(), RuntimeError> {
    for received_line in line_receiver {
        output(&received_line);
    }

    let status = wait_handle
        .join()
        .map_err(|_| RuntimeError::Panic("wait thread panicked".to_string()))?
        .map_err(|e| RuntimeError::exec_io_error(format!("git clone {}", proj_name), e))?;

    if let Some(h) = stdout_handle {
        let _ = h.join();
    }
    if let Some(h) = stderr_handle {
        let _ = h.join();
    }

    if !status.success() {
        if status.code().is_none() {
            return Err(RuntimeError::exec_io_error(
                format!("git clone {}", proj_name),
                "interrupted by signal",
            ));
        }
        return Err(RuntimeError::exec_exit_code(
            format!("git clone {}", proj_name),
            status.code(),
        ));
    }

    Ok(())
}

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
            output(&format!("skip  {} (sync=ignore)", proj_name));
            Ok(())
        }
        SyncMode::Clone => run_sync_clone(proj_name, proj, output),
    }
}

/// Git-clone (or skip if already present) a single project's repo into
/// `proj.dir`. Progress is reported through the `output` callback.
fn run_sync_clone(
    proj_name: &str,
    proj: &PlanProject,
    output: &mut dyn FnMut(&str),
) -> Result<(), RuntimeError> {
    let target_dir = PathBuf::from(&proj.dir);
    let git_dir = target_dir.join(".git");

    if git_dir.exists() {
        output(&format!("exists  {} → {}", proj_name, target_dir.display()));
        return Ok(());
    }

    output(&format!("clone  {} → {}", proj_name, target_dir.display()));

    let target_dir_str = target_dir.to_string_lossy().to_string();
    let args = match &proj.branch {
        Some(branch) => vec!["clone", "-b", branch, &proj.url, &target_dir_str],
        None => vec!["clone", &proj.url, &target_dir_str],
    };

    let child = Command::new("git")
        .args(&args)
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .map_err(|e| RuntimeError::exec_io_error(format!("git clone {}", proj_name), e))?;

    let (line_receiver, stdout_handle, stderr_handle, wait_handle) =
        spawn_git_output_readers(child);

    drain_and_check_git_output(
        proj_name,
        line_receiver,
        stdout_handle,
        stderr_handle,
        wait_handle,
        output,
    )
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
pub fn run_sync_for_projects(
    mut projects: HashMap<String, PlanProject>,
    chain_pairs: Vec<(String, Vec<String>)>,
) -> miette::Result<()> {
    let name_indices: Vec<(String, usize)> = chain_pairs
        .iter()
        .enumerate()
        .map(|(i, (name, _))| (name.clone(), i))
        .collect();

    if runner::run_tui_with_sync(chain_pairs, move |tx| async move {
        let mut had_errors = false;
        let mut join_handles = Vec::new();

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
                crate::runner::sync::sync_project_with_callback(
                    &project_name_clone,
                    &project,
                    |line: &str| {
                        runner::send_tui_event(
                            &tx_cb,
                            TuiEvent::AppendOutput(project_index, line.to_string()),
                        );
                    },
                )
            });

            join_handles.push((project_index, handle));
        }

        for (i, handle) in join_handles {
            let outcome = match handle.await {
                Ok(Ok(())) => TaskOutcome::Success,
                Ok(Err(e)) => TaskOutcome::Error(e),
                Err(e) => TaskOutcome::Panic(e),
            };
            if report_task_outcome(&tx, i, outcome) {
                had_errors = true;
            }
        }

        if had_errors {
            Err(miette::miette!("One or more projects failed to sync"))
        } else {
            Ok(())
        }
    })
    .is_err()
    {
        std::process::exit(1);
    }
    Ok(())
}
