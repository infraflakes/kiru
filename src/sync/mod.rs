use crate::config::{Config, ConfigError, Project};
use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::process::Command;

pub fn sync_all(cfg: &Config, writer: &mut dyn Write) -> Result<(), ConfigError> {
    fs::create_dir_all(&cfg.sanctuary).map_err(|e| {
        ConfigError::Validation(format!("cannot create sanctuary {}: {}", cfg.sanctuary, e))
    })?;

    for proj in cfg.projects.values() {
        sync_project_inner(&cfg.sanctuary, proj, &mut |line: &str| {
            let _ = writeln!(writer, "  {}", line);
        })?;
    }

    let mut buf = Vec::new();
    warn_unknown_repos_inner(&cfg.sanctuary, &cfg.projects, &mut |line: &str| {
        buf.push(line.to_string());
    })?;
    for line in buf {
        writeln!(writer, "{}", line)
            .map_err(|e| ConfigError::Validation(format!("write error: {}", e)))?;
    }

    Ok(())
}

fn sync_project_inner(
    sanctuary: &str,
    proj: &Project,
    output: &mut dyn FnMut(&str),
) -> Result<(), ConfigError> {
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

    let mut child = Command::new("git")
        .args(&args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .map_err(|e| {
            ConfigError::Validation(format!(
                "failed to clone {}: git command failed: {}",
                proj.name, e
            ))
        })?;

    if let Some(stdout) = child.stdout.take() {
        for line in std::io::BufReader::new(stdout).lines() {
            let line = line.map_err(|e| {
                ConfigError::Validation(format!("failed to read clone output: {}", e))
            })?;
            output(&format!("    {}", line));
        }
    }
    if let Some(stderr) = child.stderr.take() {
        for line in std::io::BufReader::new(stderr).lines() {
            let line = line.map_err(|e| {
                ConfigError::Validation(format!("failed to read clone output: {}", e))
            })?;
            output(&line);
        }
    }

    let status = child
        .wait()
        .map_err(|e| ConfigError::Validation(format!("failed to clone {}: {}", proj.name, e)))?;

    if !status.success() {
        return Err(ConfigError::Validation(format!(
            "failed to clone {}",
            proj.name
        )));
    }

    Ok(())
}

pub fn sync_project_with_callback(
    sanctuary: &str,
    proj: &Project,
    mut output_cb: impl FnMut(&str),
) -> Result<(), ConfigError> {
    sync_project_inner(sanctuary, proj, &mut output_cb)
}

fn warn_unknown_repos_inner(
    sanctuary: &str,
    projects: &std::collections::HashMap<String, Project>,
    output: &mut dyn FnMut(&str),
) -> Result<(), ConfigError> {
    let mut known_dirs = std::collections::HashSet::new();
    for proj in projects.values() {
        known_dirs.insert(&proj.dir);
    }

    let entries = match fs::read_dir(sanctuary) {
        Ok(e) => e,
        Err(_) => return Ok(()),
    };

    for entry in entries {
        let entry = entry
            .map_err(|e| ConfigError::Validation(format!("failed to read sanctuary: {}", e)))?;
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
) -> Result<(), ConfigError> {
    let mut warnings = Vec::new();
    warn_unknown_repos_inner(sanctuary, projects, &mut |line: &str| {
        warnings.push(line.to_string());
    })?;
    if !warnings.is_empty() {
        return Err(ConfigError::Validation(warnings.join("\n")));
    }
    Ok(())
}
