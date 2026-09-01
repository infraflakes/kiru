//! `kiru.toml` configuration: the single mandatory source of truth for
//! machine-level config (shell, timeout, repos). Read before anything else;
//! the DSL (`main.kiru`) is pure behavior.

use serde::Deserialize;
use std::path::{Path, PathBuf};

/// The top-level `kiru.toml` schema.
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct KiruToml {
    /// Schema version (must be 1).
    #[serde(default = "default_version")]
    pub(crate) version: u8,

    /// Shell binary name used for `$(cmd)` substitution and `exec`.
    /// Defaults to `"sh"` when absent.
    #[serde(default = "default_shell")]
    pub(crate) shell: String,

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

    /// Sync strategy: `"clone"` (default) or `"ignore"`.
    #[serde(default = "default_strategy")]
    pub(crate) strategy: String,
}

fn default_version() -> u8 {
    1
}

fn default_shell() -> String {
    "sh".to_string()
}

fn default_strategy() -> String {
    "clone".to_string()
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

/// Load and validate `kiru.toml` from the config directory. Returns a
/// hard error if the file is missing or malformed.
pub(crate) fn load_kiru_toml() -> Result<KiruToml, String> {
    let path = get_kiru_toml_path();
    let text = std::fs::read_to_string(&path)
        .map_err(|e| format!("failed to read {}: {}", path.display(), e))?;
    let config: KiruToml =
        toml::from_str(&text).map_err(|e| format!("failed to parse {}: {}", path.display(), e))?;
    validate_kiru_toml(&config)?;
    Ok(config)
}

/// Validate a `KiruToml` after parsing.
fn validate_kiru_toml(config: &KiruToml) -> Result<(), String> {
    if config.version != 1 {
        return Err(format!(
            "unsupported kiru.toml version: {} (expected 1)",
            config.version
        ));
    }
    for repo in &config.repos {
        if repo.strategy != "clone" && repo.strategy != "ignore" {
            return Err(format!(
                "repo '{}': unknown strategy '{}' (expected 'clone' or 'ignore')",
                repo.name, repo.strategy
            ));
        }
    }
    if let Some(timeout) = config.timeout
        && timeout == 0
    {
        return Err("timeout must be greater than zero when set".to_string());
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
    fn test_validate_default_strategy() {
        let config = KiruToml {
            version: 1,
            shell: "sh".to_string(),
            timeout: None,
            repos: vec![Repo {
                name: "test".to_string(),
                url: "https://example.com".to_string(),
                dir: "~/test".to_string(),
                branch: String::new(),
                strategy: "clone".to_string(),
            }],
        };
        assert!(validate_kiru_toml(&config).is_ok());
    }

    #[test]
    fn test_validate_bad_strategy() {
        let config = KiruToml {
            version: 1,
            shell: "sh".to_string(),
            timeout: None,
            repos: vec![Repo {
                name: "test".to_string(),
                url: String::new(),
                dir: String::new(),
                branch: String::new(),
                strategy: "pull".to_string(),
            }],
        };
        assert!(validate_kiru_toml(&config).is_err());
    }

    #[test]
    fn test_validate_zero_timeout() {
        let config = KiruToml {
            version: 1,
            shell: "sh".to_string(),
            timeout: Some(0),
            repos: vec![],
        };
        assert!(validate_kiru_toml(&config).is_err());
    }
}
