//! direnv integration: per-project opt-in wrapping of `$(command)` execution
//! in `direnv exec <dir>` so the project environment applies to shell
//! commands.
//!
//! A project that sets `direnv = true` in its `kiru.toml` entry opts into
//! trusting the repository's rc completely: before a function of the
//! project runs, kiru executes `direnv allow <dir>` so the rc is always
//! approved, then every shell command is prefixed with `direnv exec <dir>`.
//!
//! There are no further checks on kiru's side - no binary lookup, no
//! `.envrc` probing, no allow-status sniffing. Whatever goes wrong belongs
//! to direnv and surfaces as direnv's own error: a missing binary fails
//! the spawn, a failing rc fails the command, and a directory without an
//! rc runs the command unchanged inside `direnv exec`. Repos without the
//! flag run their commands plain.

use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;

use super::error::RuntimeError;
use super::subprocess::{self, RunKillSwitch};

/// The direnv binary name, used for both `allow` and `exec`.
pub(crate) const DIRENV_PROGRAM: &str = "direnv";

/// Approve the rc of `project_dir` with `direnv allow <dir>`, so the later
/// `direnv exec` wrapping is never rejected as untrusted. direnv's own
/// output is folded into the error message when the command fails: a project
/// that opted into direnv must have a working direnv, there is no fallback
/// to plain execution. `env_overrides` follows the spawn convention of
/// [`subprocess::run_subprocess`].
pub(crate) fn allow_project_env(
    project_dir: &Path,
    timeout: Option<Duration>,
    kill: Option<&RunKillSwitch>,
    env_overrides: Option<&HashMap<String, String>>,
) -> Result<(), RuntimeError> {
    let program = DIRENV_PROGRAM;
    let dir = project_dir.to_string_lossy();
    let invocation = format!("{program} allow {dir}");
    let mut direnv_report = String::new();
    let exit = subprocess::run_subprocess(
        &invocation,
        &[program, "allow", dir.as_ref()],
        None,
        env_overrides,
        timeout,
        kill,
        &mut |line| match line {
            subprocess::SubprocessLine::Stdout(text) | subprocess::SubprocessLine::Stderr(text) => {
                direnv_report.push_str(&text);
                direnv_report.push('\n');
            }
        },
    )
    .map_err(|e| match e {
        subprocess::SubprocessError::Timeout { command, .. } => RuntimeError::Timeout {
            cmd: command,
            secs: timeout.map_or(0, |d| d.as_secs()),
        },
        other => RuntimeError::exec_io_error(&invocation, other),
    })?;
    // A stop from the run's fail-fast or a keyboard cancel is not an
    // independent allow failure.
    if exit.killed_by_switch {
        return Err(RuntimeError::Cancelled(
            "stopped because another chain failed".to_string(),
        ));
    }
    if exit.status.success() {
        return Ok(());
    }
    // direnv reports on stderr (or stdout); without any output all that is
    // left to say is how the process died.
    let detail = match direnv_report.trim().is_empty() {
        true => subprocess::describe_exit_failure(&exit.status),
        false => direnv_report.trim().to_string(),
    };
    Err(RuntimeError::exec_io_error(&invocation, detail))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A fake `direnv` executable whose behavior the test controls through
    /// an environment variable, so the allow flow is exercised end to end
    /// without depending on a real direnv install.
    fn fake_direnv_env(bin_dir: &Path, fake_exit: Option<&str>) -> HashMap<String, String> {
        let script = r#"#!/bin/sh
echo "fake direnv: $*"
exit ${FAKE_DIRENV_EXIT:-0}
"#;
        let bin = bin_dir.join(DIRENV_PROGRAM);
        std::fs::write(&bin, script).unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&bin, std::fs::Permissions::from_mode(0o755)).unwrap();

        let mut env = HashMap::new();
        env.insert("PATH".to_string(), bin_dir.to_string_lossy().into_owned());
        if let Some(fake_exit) = fake_exit {
            env.insert("FAKE_DIRENV_EXIT".to_string(), fake_exit.to_string());
        }
        env
    }

    #[test]
    fn allow_succeeds_when_direnv_exits_zero() {
        let bin_dir = tempfile::tempdir().unwrap();
        let env = fake_direnv_env(bin_dir.path(), None);
        let dir = tempfile::tempdir().unwrap();
        allow_project_env(dir.path(), None, None, Some(&env)).unwrap();
    }

    #[test]
    fn allow_fails_loudly_with_direnv_output() {
        let bin_dir = tempfile::tempdir().unwrap();
        let env = fake_direnv_env(bin_dir.path(), Some("1"));
        let dir = tempfile::tempdir().unwrap();
        let error = allow_project_env(dir.path(), None, None, Some(&env)).unwrap_err();
        let rendered = error.to_string();
        assert!(rendered.contains("fake direnv:"), "{rendered}");
        assert!(rendered.contains("allow"), "{rendered}");
    }

    #[test]
    fn allow_fails_loudly_with_the_exit_cause_when_silent() {
        let bin_dir = tempfile::tempdir().unwrap();
        let env = fake_direnv_env(bin_dir.path(), None);
        // Silence the fake binary: the error must fall back to how the
        // process died.
        let bin = bin_dir.path().join(DIRENV_PROGRAM);
        std::fs::write(&bin, "#!/bin/sh\nexit 1\n").unwrap();
        let dir = tempfile::tempdir().unwrap();
        let error = allow_project_env(dir.path(), None, None, Some(&env)).unwrap_err();
        assert!(error.to_string().contains("exited with code 1"), "{error}");
    }
}
