use crate::config::Project;
use crate::runner::error::RuntimeError;
use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn sync_project_inner(
    sanctuary: &str,
    proj: &Project,
    output: &mut dyn FnMut(&str),
) -> Result<(), RuntimeError> {
    if proj.sync == "ignore" {
        output(&format!("skip  {} (sync=ignore)", proj.name));
        return Ok(());
    }

    let target_dir = PathBuf::from(sanctuary).join(&proj.dir);
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
    use std::thread;

    let mut child = Command::new("git")
        .args(&args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| RuntimeError::exec_io_error(format!("git clone {}", proj.name), e))?;

    let stdout_handle = child.stdout.take().map(|s| {
        thread::spawn(move || {
            let mut lines = Vec::new();
            for line in std::io::BufReader::new(s).lines() {
                let line = line.map_err(RuntimeError::Io)?;
                lines.push(format!("    {}", line));
            }
            Ok::<_, RuntimeError>(lines)
        })
    });
    let stderr_handle = child.stderr.take().map(|s| {
        thread::spawn(move || {
            let mut lines = Vec::new();
            for line in std::io::BufReader::new(s).lines() {
                let line = line.map_err(RuntimeError::Io)?;
                lines.push(line);
            }
            Ok::<_, RuntimeError>(lines)
        })
    });

    let status = child
        .wait()
        .map_err(|e| RuntimeError::exec_io_error(format!("git clone {}", proj.name), e))?;

    if let Some(h) = stdout_handle {
        for line in h
            .join()
            .map_err(|_| RuntimeError::Panic("stdout thread panicked".to_string()))??
        {
            output(&line);
        }
    }
    if let Some(h) = stderr_handle {
        for line in h
            .join()
            .map_err(|_| RuntimeError::Panic("stderr thread panicked".to_string()))??
        {
            output(&line);
        }
    }

    if !status.success() {
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

fn warn_unknown_repos_inner(
    sanctuary: &str,
    projects: &std::collections::HashMap<String, Project>,
    output: &mut dyn FnMut(&str),
) -> Result<(), RuntimeError> {
    let mut known_dirs = std::collections::HashSet::new();
    for proj in projects.values() {
        known_dirs.insert(&proj.dir);
    }

    let entries = match fs::read_dir(sanctuary) {
        Ok(e) => e,
        Err(_) => return Ok(()),
    };

    for entry in entries {
        let entry = entry.map_err(RuntimeError::Io)?;
        if !entry.path().is_dir() {
            continue;
        }
        let name = entry.file_name();
        let name_str = name.to_string_lossy().to_string();
        if known_dirs.contains(&name_str) {
            continue;
        }
        let git_dir = PathBuf::from(sanctuary).join(&name_str).join(".git");
        if git_dir.exists() {
            output(&format!(
                "  warn  {}/{} is a git repo not in your config",
                sanctuary, name_str
            ));
        }
    }

    Ok(())
}

pub fn warn_unknown_repos(
    sanctuary: &str,
    projects: &std::collections::HashMap<String, Project>,
) -> Result<(), RuntimeError> {
    let mut warnings = Vec::new();
    warn_unknown_repos_inner(sanctuary, projects, &mut |line: &str| {
        warnings.push(line.to_string());
    })?;
    if !warnings.is_empty() {
        return Err(RuntimeError::Other(warnings.join("\n")));
    }
    Ok(())
}
