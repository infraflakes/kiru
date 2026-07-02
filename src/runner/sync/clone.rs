use crate::compiler::{Project, SyncMode};
use crate::runner::error::RuntimeError;
use crate::runner::{self, TaskStatus, TuiEvent};
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Command;

/// Clone (or skip) a single project's git repo into `proj.dir`.
/// Reports progress through the `output` callback.
fn sync_project_inner(
    proj_name: &str,
    proj: &Project,
    output: &mut dyn FnMut(&str),
) -> Result<(), RuntimeError> {
    if proj.sync == SyncMode::Ignore {
        output(&format!("skip  {} (sync=ignore)", proj_name));
        return Ok(());
    }

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

    use std::io::BufRead;
    use std::process::Stdio;
    use std::sync::mpsc;
    use std::thread;

    let mut child = Command::new("git")
        .args(&args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| RuntimeError::exec_io_error(format!("git clone {}", proj_name), e))?;

    // Stream git output in real-time: reader threads send lines through a channel,
    // main thread drains them while child.wait() runs in a background thread.
    let (line_sender, line_receiver) = mpsc::channel::<String>();

    let stdout_handle = child.stdout.take().map(|stdout_stream| {
        let line_sender = line_sender.clone();
        thread::spawn(move || {
            for line in std::io::BufReader::new(stdout_stream)
                .lines()
                .map_while(Result::ok)
            {
                let _ = line_sender.send(format!("    {}", line));
            }
        })
    });
    let stderr_handle = child.stderr.take().map(|stderr_stream| {
        let line_sender = line_sender.clone();
        thread::spawn(move || {
            for line in std::io::BufReader::new(stderr_stream)
                .lines()
                .map_while(Result::ok)
            {
                let _ = line_sender.send(line);
            }
        })
    });
    drop(line_sender);

    let wait_handle = thread::spawn(move || child.wait());

    for received_line in line_receiver {
        output(&received_line);
    }

    let status = wait_handle
        .join()
        .map_err(|_| RuntimeError::Panic("wait thread panicked".to_string()))?
        .map_err(|e| RuntimeError::exec_io_error(format!("git clone {}", proj_name), e))?;

    if let Some(thread_handle) = stdout_handle {
        let _ = thread_handle.join();
    }
    if let Some(thread_handle) = stderr_handle {
        let _ = thread_handle.join();
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

/// Synchronize a single project into `proj.dir`.
/// Accepts an output callback that receives progress lines for display or forwarding.
pub fn sync_project_with_callback(
    proj_name: &str,
    proj: &Project,
    mut output_cb: impl FnMut(&str),
) -> Result<(), RuntimeError> {
    sync_project_inner(proj_name, proj, &mut output_cb)
}

/// Run sync for all projects through the TUI.
pub fn run_sync_for_projects(
    mut projects: HashMap<String, Project>,
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
            match handle.await {
                Ok(Ok(())) => {
                    runner::send_tui_event(&tx, TuiEvent::UpdateStatus(i, TaskStatus::Success));
                }
                Ok(Err(e)) => {
                    had_errors = true;
                    runner::send_tui_event(&tx, TuiEvent::AppendOutput(i, format!("Error: {}", e)));
                    runner::send_tui_event(&tx, TuiEvent::UpdateStatus(i, TaskStatus::Error));
                }
                Err(e) => {
                    had_errors = true;
                    runner::send_tui_event(
                        &tx,
                        TuiEvent::AppendOutput(i, format!("Task panicked: {}", e)),
                    );
                    runner::send_tui_event(&tx, TuiEvent::UpdateStatus(i, TaskStatus::Error));
                }
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
