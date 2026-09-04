//! direnv integration: opt-in wrapping of `$(command)` execution in
//! `direnv exec <dir>` so project environments apply to shell commands.
//!
//! Detection is deliberately strict and side-effect free: `direnv exec` is
//! only used when `direnv status` (run inside the target directory) reports
//! the rc found for that directory as allowed. The decision is cached per
//! directory so the check runs at most once per directory per run.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// The direnv binary name, looked up on `PATH` at spawn time. Single home:
/// both the detection probe and the wrapped argv reference it.
pub(crate) const DIRENV_PROGRAM: &str = "direnv";

/// Decides per directory whether commands should run via `direnv exec`.
///
/// Enabled only through `direnv = true` in `kiru.toml`; the default is off.
/// Each directory's verdict comes from a single `direnv status` invocation
/// and is cached for the rest of the run.
#[derive(Debug)]
pub(crate) struct DirenvState {
    /// Off unless `direnv = true`: no detection runs, commands stay unwrapped.
    enabled: bool,
    /// Detection verdicts keyed by absolute directory. `true` = the loaded rc
    /// is allowed, `false` = no direnv / not allowed / denied / old version.
    allowed_by_dir: Mutex<HashMap<PathBuf, bool>>,
}

impl DirenvState {
    /// Build the state for the `direnv` kiru.toml flag.
    pub(crate) fn new(enabled: bool) -> Self {
        DirenvState {
            enabled,
            allowed_by_dir: Mutex::new(HashMap::new()),
        }
    }

    /// Whether commands in `cwd` should be wrapped in `direnv exec <cwd>`.
    pub(crate) fn should_wrap(&self, cwd: &Path) -> bool {
        if !self.enabled {
            return false;
        }
        let mut cache = self
            .allowed_by_dir
            .lock()
            .unwrap_or_else(|e| e.into_inner());
        *cache
            .entry(cwd.to_path_buf())
            .or_insert_with(|| detect_direnv_allowed(cwd))
    }
}

/// Run `direnv status` inside `cwd` and report whether the rc found for
/// that directory is allowed. Any failure (binary missing from PATH, spawn
/// error, non-zero exit) means "do not wrap": direnv integration degrades
/// to plain commands.
fn detect_direnv_allowed(cwd: &Path) -> bool {
    let output = match std::process::Command::new(DIRENV_PROGRAM)
        .arg("status")
        .current_dir(cwd)
        .output()
    {
        Ok(output) if output.status.success() => output,
        _ => return false,
    };
    status_says_allowed(&String::from_utf8_lossy(&output.stdout))
}

/// Decide from `direnv status` output whether `direnv exec <cwd>` will load.
///
/// direnv 2.33.0 changed the encoding to numbers, counterintuitively:
/// 0 = allowed, 1 = not allowed, 2 = denied (direnv#1223). Older versions
/// print `true`/`false` instead. Matching the exact `0` line keeps both
/// non-allowed states and unsupported old versions on the plain path.
///
/// The `Found RC` section describes the status process's working directory
/// (what `direnv exec <cwd>` would load), while `Loaded RC` describes the rc
/// inherited through the environment from the parent shell. The spawned
/// check cares only about the target directory, hence `Found`.
fn status_says_allowed(status_output: &str) -> bool {
    status_output
        .lines()
        .any(|line| line.trim() == "Found RC allowed 0")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_allowed_zero_wraps() {
        assert!(status_says_allowed("Found RC allowed 0"));
    }

    #[test]
    fn status_not_allowed_and_denied_do_not_wrap() {
        assert!(!status_says_allowed("Found RC allowed 1"));
        assert!(!status_says_allowed("Found RC allowed 2"));
    }

    #[test]
    fn status_old_boolean_format_does_not_wrap() {
        // Pre-2.33 direnv prints booleans; unsupported, stay unwrapped.
        assert!(!status_says_allowed("Found RC allowed true"));
        assert!(!status_says_allowed("Found RC allowed false"));
    }

    #[test]
    fn status_full_allowed_output_wraps() {
        let output = "\
direnv 2.37.1
Loaded RC path /somewhere/else/.envrc
Loaded RC allowed 0
Found RC path /tmp/project/.envrc
Found RC allowed 0
";
        assert!(status_says_allowed(output));
    }

    #[test]
    fn status_found_not_allowed_does_not_wrap_even_if_loaded_allowed() {
        // The loaded rc belongs to the parent shell's directory; only the
        // rc found for the target directory decides.
        let output = "\
Loaded RC path /parent/shell/.envrc
Loaded RC allowed 0
Found RC path /tmp/project/.envrc
Found RC allowed 1
";
        assert!(!status_says_allowed(output));
    }

    #[test]
    fn status_no_rc_found_does_not_wrap() {
        let output = "direnv 2.37.1\nNo .envrc or .env found\n";
        assert!(!status_says_allowed(output));
    }

    #[test]
    fn status_empty_output_does_not_wrap() {
        assert!(!status_says_allowed(""));
    }

    #[test]
    fn disabled_state_never_wraps() {
        let state = DirenvState::new(false);
        assert!(!state.should_wrap(Path::new("/")));
    }
}
