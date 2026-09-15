//! `kiru.toml` holds per-machine settings (repos, shell, timeout).
//! Kept separate from the DSL (`main.kiru`) so the DSL stays portable
//! and only the machine-specific bits live here.

use serde::Deserialize;
use std::path::{Path, PathBuf};

/// The top-level `kiru.toml` schema. Unknown keys are rejected so a typo
/// (`shelll`, a retired top-level `direnv`, ...) is a clear error instead
/// of a silently ignored setting.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct KiruToml {
    /// Shell binary used for `$(cmd)` substitution and `exec`. Absent
    /// means the default (`sh`) applies at the use sites.
    #[serde(default)]
    pub(crate) shell: Option<String>,

    /// Global timeout in seconds for `$(cmd)` substitution. `None` means
    /// no timeout (commands run indefinitely).
    #[serde(default)]
    pub(crate) timeout: Option<u64>,

    /// Repository declarations that `kiru sync` clones/pulls and that the
    /// executor uses to resolve project working directories.
    #[serde(default)]
    pub(crate) repos: Vec<Repo>,
}

/// A single repository declaration in `kiru.toml`.
#[derive(Debug, Clone, Deserialize, PartialEq)]
pub(crate) struct Repo {
    /// Project name, matching `pr <name>` in the DSL.
    pub(crate) name: String,

    /// Git remote URL. Empty string means no remote (skip sync).
    #[serde(default)]
    pub(crate) url: String,

    /// Local directory where the repo is cloned. Supports `~` expansion.
    /// Empty string means no project metadata (commands run in invocation cwd).
    #[serde(default)]
    pub(crate) dir: String,

    /// Branch to clone/pull. Empty string means the default branch.
    #[serde(default)]
    pub(crate) branch: String,

    /// Opt-in direnv integration for this repository: before a function of
    /// the project runs, kiru executes `direnv allow <dir>` so the rc is
    /// always approved, then every shell command of the project is wrapped
    /// in `direnv exec <dir>`. There are no further checks on kiru's side:
    /// a missing direnv binary, a failing rc, or a directory without an
    /// `.envrc` fails the command through direnv itself. Disabled by
    /// default; commands of unflagged repos run plain.
    #[serde(default)]
    pub(crate) direnv: bool,
}

/// Expand `~` and `$HOME` in a path string. `~` is replaced with the user's
/// home directory; `$HOME` is expanded from the process environment.
fn expand_home(path: &str) -> String {
    if let Some(rest) = path.strip_prefix("~/")
        && let Some(home) = dirs::home_dir()
    {
        return home.join(rest).to_string_lossy().to_string();
    }
    if path == "~" {
        return dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .to_string_lossy()
            .to_string();
    }
    if let Some(rest) = path.strip_prefix("$HOME/")
        && let Ok(home) = std::env::var("HOME")
    {
        return Path::new(&home).join(rest).to_string_lossy().to_string();
    }
    path.to_string()
}

/// Load and validate `kiru.toml` from an explicit path. Callers resolve the
/// path first (`-c` override or the canonical `~/.config/kiru/kiru.toml`).
pub(crate) fn load_kiru_toml_at(path: &Path) -> Result<KiruToml, String> {
    let text = std::fs::read_to_string(path)
        .map_err(|e| format!("failed to read {}: {}", path.display(), e))?;
    let config: KiruToml =
        toml::from_str(&text).map_err(|e| format!("failed to parse {}: {}", path.display(), e))?;
    validate_kiru_toml(&config)?;
    Ok(config)
}

/// Load `kiru.toml` when it exists; a missing file is the all-defaults
/// configuration (commands run at the invocation cwd, shell `sh`, no
/// timeout, no direnv anywhere). A file that exists but is malformed is a
/// hard error.
pub(crate) fn load_kiru_toml_or_default(path: &Path) -> Result<KiruToml, String> {
    if !path.exists() {
        return Ok(KiruToml::default());
    }
    load_kiru_toml_at(path)
}

/// Validate a `KiruToml` after parsing.
fn validate_kiru_toml(config: &KiruToml) -> Result<(), String> {
    if let Some(timeout) = config.timeout
        && timeout == 0
    {
        return Err("timeout must be greater than zero when set".to_string());
    }
    for repo in &config.repos {
        if repo.direnv && repo.dir.is_empty() {
            return Err(format!(
                "project {}: direnv requires a dir (direnv exec runs commands there)",
                repo.name
            ));
        }
    }
    Ok(())
}

/// Expand `~` in all repo `dir` fields. Must be called after loading
/// and before using the paths.
pub(crate) fn expand_repo_dirs(config: &mut KiruToml) {
    for repo in &mut config.repos {
        repo.dir = expand_home(&repo.dir);
    }
}

/// Find the canonical `kiru.toml` path: `~/.config/kiru/kiru.toml`.
pub(crate) fn get_kiru_toml_path() -> PathBuf {
    super::kiru_config_dir().join("kiru.toml")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_expand_home_tilde_slash() {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        assert_eq!(expand_home("~/foo"), home.join("foo").to_string_lossy());
    }

    #[test]
    fn test_expand_home_tilde_only() {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        assert_eq!(expand_home("~"), home.to_string_lossy());
    }

    #[test]
    fn test_expand_home_dollar() {
        let home = std::env::var("HOME").unwrap_or_default();
        assert_eq!(
            expand_home("$HOME/foo"),
            Path::new(&home).join("foo").to_string_lossy()
        );
    }

    #[test]
    fn test_expand_home_noop() {
        assert_eq!(expand_home("/absolute/path"), "/absolute/path");
    }

    #[test]
    fn test_missing_toml_is_all_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("kiru.toml");
        let config = load_kiru_toml_or_default(&path).unwrap();
        assert_eq!(config.shell, None);
        assert_eq!(config.timeout, None);
        assert!(config.repos.is_empty());
    }

    #[test]
    fn test_malformed_toml_still_errors() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("kiru.toml");
        std::fs::write(&path, "not [valid toml").unwrap();
        assert!(load_kiru_toml_or_default(&path).is_err());
    }

    #[test]
    fn test_validate_zero_timeout() {
        let config = KiruToml {
            shell: None,
            timeout: Some(0),
            repos: vec![],
        };
        assert!(validate_kiru_toml(&config).is_err());
    }

    #[test]
    fn test_repo_direnv_defaults_to_disabled() {
        // A repo without a `direnv` key must deserialize with the
        // integration off.
        let config: KiruToml = toml::from_str(
            r#"
            [[repos]]
            name = "todo"
            dir = "~/projects/todo"
            "#,
        )
        .unwrap();
        assert!(!config.repos[0].direnv);
    }

    #[test]
    fn test_repo_direnv_opt_in_parses() {
        let config: KiruToml = toml::from_str(
            r#"
            [[repos]]
            name = "todo"
            dir = "~/projects/todo"
            direnv = true
            "#,
        )
        .unwrap();
        assert!(config.repos[0].direnv);
    }

    #[test]
    fn test_unknown_top_level_key_is_rejected() {
        // A retired top-level `direnv` (now per repo) or any typo must be
        // a hard error, not a silently ignored setting.
        let result = toml::from_str::<KiruToml>("direnv = true");
        assert!(result.is_err());
    }

    #[test]
    fn test_repo_direnv_without_dir_is_rejected() {
        let config = KiruToml {
            shell: None,
            timeout: None,
            repos: vec![Repo {
                name: "todo".to_string(),
                url: String::new(),
                dir: String::new(),
                branch: String::new(),
                direnv: true,
            }],
        };
        assert!(validate_kiru_toml(&config).is_err());
    }

    #[test]
    fn test_absent_options_stay_none() {
        // Whatever is not in the file stays unset: status renders defaults
        // invisibly, so the loader must not fill them in.
        let config: KiruToml = toml::from_str("").unwrap();
        assert_eq!(config.shell, None);
        assert_eq!(config.timeout, None);
        assert!(config.repos.is_empty());
    }

    #[test]
    fn test_shell_only_when_present() {
        let config: KiruToml = toml::from_str("shell = \"zsh\"").unwrap();
        assert_eq!(config.shell, Some("zsh".to_string()));
    }
}
