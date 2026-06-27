use crate::compiler::{Project, SyncMode};
use crate::runner::error::RuntimeError;
use std::path::PathBuf;
use std::process::Command;

fn sync_project_inner(
    sanctuary: &str,
    proj: &Project,
    output: &mut dyn FnMut(&str),
) -> Result<(), RuntimeError> {
    if proj.sync == SyncMode::Ignore {
        output(&format!("skip  {} (sync=ignore)", proj.name));
        return Ok(());
    }

    let target_dir = PathBuf::from(sanctuary).join(proj.dir.trim_start_matches('/'));
    let git_dir = target_dir.join(".git");

    if git_dir.exists() {
        output(&format!("exists  {} → {}", proj.name, target_dir.display()));
        return Ok(());
    }

    output(&format!("clone  {} → {}", proj.name, target_dir.display()));

    let target_dir_str = target_dir.to_string_lossy().to_string();
    let args = if proj.branch.is_empty() {
        vec!["clone", &proj.url, &target_dir_str]
    } else {
        vec!["clone", "-b", &proj.branch, &proj.url, &target_dir_str]
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
        .map_err(|e| RuntimeError::exec_io_error(format!("git clone {}", proj.name), e))?;

    // Stream git output in real-time: reader threads send lines through a channel,
    // main thread drains them while child.wait() runs in a background thread.
    let (tx, rx) = mpsc::channel::<String>();

    let stdout_handle = child.stdout.take().map(|s| {
        let tx = tx.clone();
        thread::spawn(move || {
            for line in std::io::BufReader::new(s).lines().map_while(Result::ok) {
                let _ = tx.send(format!("    {}", line));
            }
        })
    });
    let stderr_handle = child.stderr.take().map(|s| {
        let tx = tx.clone();
        thread::spawn(move || {
            for line in std::io::BufReader::new(s).lines().map_while(Result::ok) {
                let _ = tx.send(line);
            }
        })
    });
    drop(tx);

    let wait_handle = thread::spawn(move || child.wait());

    for line in rx {
        output(&line);
    }

    let status = wait_handle
        .join()
        .map_err(|_| RuntimeError::Panic("wait thread panicked".to_string()))?
        .map_err(|e| RuntimeError::exec_io_error(format!("git clone {}", proj.name), e))?;

    if let Some(h) = stdout_handle {
        let _ = h.join();
    }
    if let Some(h) = stderr_handle {
        let _ = h.join();
    }

    if !status.success() {
        if status.code().is_none() {
            return Err(RuntimeError::exec_io_error(
                format!("git clone {}", proj.name),
                "interrupted by signal",
            ));
        }
        return Err(RuntimeError::exec_exit_code(
            format!("git clone {}", proj.name),
            status.code(),
        ));
    }

    Ok(())
}

pub fn sync_project_with_callback(
    sanctuary: &str,
    proj: &Project,
    mut output_cb: impl FnMut(&str),
) -> Result<(), RuntimeError> {
    sync_project_inner(sanctuary, proj, &mut output_cb)
}
