//! `kiru.toml` is the source of truth for every command: it declares
//! profiles, and each profile is a complete unit - where to compile from
//! (`source`), where the compiled IR goes (`output`), the shell, the
//! timeout, and the projects. The selected profile is chosen with
//! `-p/--profile`; `-c/--config` points at the file itself (defaulting to
//! `~/.config/kiru/kiru.toml`).

use serde::Deserialize;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

/// The top-level `kiru.toml` schema: the only key is the profile table.
/// Unknown keys are rejected so a typo (`proifle`, a retired top-level
/// `direnv`, ...) is a clear error instead of a silently ignored setting.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct KiruToml {
    /// Profiles keyed by name. The name is the table key, so TOML itself
    /// rejects duplicate profile declarations.
    #[serde(default)]
    pub(crate) profile: BTreeMap<String, Profile>,
}

/// One profile: a complete declaration of a compile-and-run environment.
#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct Profile {
    /// The `.kiru` source file `kiru compile` reads. Relative to this
    /// file's directory; `~/...` and absolute paths work too.
    pub(crate) source: String,

    /// Where `kiru compile` writes the IR, filename included. `run` and
    /// `status` read the IR from exactly this path.
    pub(crate) output: String,

    /// Shell binary used for `$(cmd)` substitution and `exec`. Absent
    /// means the default (`sh`) applies at the use sites.
    #[serde(default)]
    pub(crate) shell: Option<String>,

    /// Global timeout in seconds for `$(cmd)` substitution. `None` means
    /// no timeout (commands run indefinitely).
    #[serde(default)]
    pub(crate) timeout: Option<u64>,

    /// Project declarations, keyed by project name matching
    /// `project <name>` in the DSL, so TOML itself rejects duplicates.
    #[serde(default, rename = "project")]
    pub(crate) projects: BTreeMap<String, TomlProject>,
}

/// A single project declaration inside a profile, under
/// `[profile.<name>.project.<project name>]`.
#[derive(Debug, Clone, Deserialize, PartialEq)]
#[serde(deny_unknown_fields)]
pub(crate) struct TomlProject {
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

    /// Opt-in direnv integration for this project: before a function of
    /// the project runs, kiru executes `direnv allow <dir>` so the rc is
    /// always approved, then every shell command of the project is wrapped
    /// in `direnv exec <dir>`. There are no further checks on kiru's side:
    /// a missing direnv binary, a failing rc, or a directory without an
    /// `.envrc` fails the command through direnv itself. Disabled by
    /// default; commands of unflagged projects run plain.
    #[serde(default)]
    pub(crate) direnv: bool,
}

/// A selected profile with every path resolved and validated, ready for
/// the commands to consume directly.
#[derive(Debug, Clone)]
pub(crate) struct ResolvedProfile {
    pub(crate) name: String,
    /// The `.kiru` source file, absolute.
    pub(crate) source: PathBuf,
    /// The IR output file (name included), absolute.
    pub(crate) output: PathBuf,
    pub(crate) shell: Option<String>,
    pub(crate) timeout: Option<u64>,
    /// Projects with their directories expanded to absolute paths.
    pub(crate) projects: BTreeMap<String, TomlProject>,
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

/// Resolve a profile path against the kiru.toml's directory: absolute
/// paths and `~`/`$HOME` forms pass through, anything else is joined to
/// the directory the config was read from.
fn resolve_profile_path(base_dir: &Path, path: &str) -> PathBuf {
    let expanded = expand_home(path);
    let candidate = Path::new(&expanded);
    if candidate.is_absolute() {
        candidate.to_path_buf()
    } else {
        base_dir.join(candidate)
    }
}

/// Load and validate `kiru.toml` from an explicit path. Callers resolve
/// the path first (`-c` override or the canonical `~/.config/kiru/kiru.toml`);
/// the file must exist.
pub(crate) fn load_kiru_toml_at(path: &Path) -> Result<KiruToml, String> {
    if !path.exists() {
        return Err(format!("kiru.toml not found at {}", path.display()));
    }
    let text = std::fs::read_to_string(path)
        .map_err(|e| format!("failed to read {}: {}", path.display(), e))?;
    let config: KiruToml =
        toml::from_str(&text).map_err(|e| format!("failed to parse {}: {}", path.display(), e))?;
    validate_kiru_toml(&config)?;
    Ok(config)
}

/// Validate a `KiruToml` after parsing.
fn validate_kiru_toml(config: &KiruToml) -> Result<(), String> {
    for (profile_name, profile) in &config.profile {
        if let Some(timeout) = profile.timeout
            && timeout == 0
        {
            return Err(format!(
                "profile {profile_name}: timeout must be greater than zero when set"
            ));
        }
        for (project_name, project) in &profile.projects {
            if project.direnv && project.dir.is_empty() {
                return Err(format!(
                    "profile {profile_name}: project {project_name}: direnv requires a dir (direnv exec runs commands there)"
                ));
            }
        }
    }
    Ok(())
}

/// Select and resolve one profile: the entry point every command uses.
/// Paths become absolute (relative to the kiru.toml's directory, with `~`
/// and `$HOME` expanded), project dirs are expanded, and an unknown
/// profile name is an error listing what is available.
pub(crate) fn resolve_profile(
    toml_path: &Path,
    profile_name: &str,
) -> Result<ResolvedProfile, String> {
    let config = load_kiru_toml_at(toml_path)?;
    let profile = config.profile.get(profile_name).ok_or_else(|| {
        let available = config
            .profile
            .keys()
            .cloned()
            .collect::<Vec<_>>()
            .join(", ");
        match available.is_empty() {
            true => format!(
                "profile `{profile_name}` not found: {} declares no profiles",
                toml_path.display()
            ),
            false => format!(
                "profile `{profile_name}` not found: available profiles in {}: {available}",
                toml_path.display()
            ),
        }
    })?;

    let base_dir = toml_path
        .parent()
        .map(Path::to_path_buf)
        .unwrap_or_else(|| PathBuf::from("."));
    let mut projects = profile.projects.clone();
    for project in projects.values_mut() {
        project.dir = expand_home(&project.dir);
    }
    Ok(ResolvedProfile {
        name: profile_name.to_string(),
        source: resolve_profile_path(&base_dir, &profile.source),
        output: resolve_profile_path(&base_dir, &profile.output),
        shell: profile.shell.clone(),
        timeout: profile.timeout,
        projects,
    })
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
    fn test_profile_parse_with_projects() {
        let config: KiruToml = toml::from_str(
            r#"
            [profile.ci]
            source = "main.kiru"
            output = "kirufile"
            shell = "zsh"
            timeout = 60

            [profile.default]
            source = "main.kiru"
            output = "kirufile"

            [profile.default.project.todo]
            dir = "~/projects/todo"
            direnv = true
            "#,
        )
        .unwrap();
        assert_eq!(config.profile.len(), 2);
        let default = config.profile.get("default").unwrap();
        assert_eq!(default.output, "kirufile");
        assert!(default.projects["todo"].direnv);
        assert_eq!(config.profile["ci"].shell.as_deref(), Some("zsh"));
    }

    #[test]
    fn test_unknown_top_level_key_is_rejected() {
        let result = toml::from_str::<KiruToml>("shell = \"sh\"");
        assert!(result.is_err());
    }

    #[test]
    fn test_unknown_profile_key_is_rejected() {
        let result = toml::from_str::<KiruToml>(
            r#"
            [profile.default]
            source = "main.kiru"
            output = "kirufile"
            direnv = true
            "#,
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_source_and_output_are_required() {
        let result = toml::from_str::<KiruToml>(
            r#"
            [profile.default]
            shell = "sh"
            "#,
        );
        assert!(result.is_err());
    }

    #[test]
    fn test_missing_toml_is_an_error() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("kiru.toml");
        assert!(load_kiru_toml_at(&path).is_err());
    }

    #[test]
    fn test_malformed_toml_errors() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("kiru.toml");
        std::fs::write(&path, "not [valid toml").unwrap();
        assert!(load_kiru_toml_at(&path).is_err());
    }

    #[test]
    fn test_zero_timeout_per_profile_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("kiru.toml");
        std::fs::write(
            &path,
            r#"
            [profile.ci]
            source = "m.kiru"
            output = "k"
            timeout = 0
            "#,
        )
        .unwrap();
        let error = load_kiru_toml_at(&path).unwrap_err();
        assert!(error.contains("profile ci: timeout"), "{error}");
    }

    #[test]
    fn test_direnv_without_dir_per_profile_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("kiru.toml");
        std::fs::write(
            &path,
            r#"
            [profile.ci]
            source = "m.kiru"
            output = "k"

            [profile.ci.project.todo]
            direnv = true
            "#,
        )
        .unwrap();
        let error = load_kiru_toml_at(&path).unwrap_err();
        assert!(
            error.contains("project todo: direnv requires a dir"),
            "{error}"
        );
    }

    #[test]
    fn test_resolve_profile_resolves_relative_paths_against_the_toml() {
        let dir = tempfile::tempdir().unwrap();
        let nested = dir.path().join("cfg");
        std::fs::create_dir_all(&nested).unwrap();
        let path = nested.join("kiru.toml");
        std::fs::write(
            &path,
            r#"
            [profile.ci]
            source = "src/main.kiru"
            output = "build/ir/kirufile"
            "#,
        )
        .unwrap();
        let resolved = resolve_profile(&path, "ci").unwrap();
        assert_eq!(resolved.source, nested.join("src/main.kiru"));
        assert_eq!(resolved.output, nested.join("build/ir/kirufile"));
    }

    #[test]
    fn test_resolve_profile_expands_home_and_accepts_absolute() {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("kiru.toml");
        std::fs::write(
            &path,
            r#"
            [profile.ci]
            source = "~/main.kiru"
            output = "/tmp/kirufile"
            "#,
        )
        .unwrap();
        let resolved = resolve_profile(&path, "ci").unwrap();
        assert_eq!(resolved.source, home.join("main.kiru"));
        assert_eq!(resolved.output, PathBuf::from("/tmp/kirufile"));
    }

    #[test]
    fn test_unknown_profile_lists_the_available_ones() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("kiru.toml");
        std::fs::write(
            &path,
            r#"
            [profile.ci]
            source = "m.kiru"
            output = "k"
            "#,
        )
        .unwrap();
        let error = resolve_profile(&path, "missing").unwrap_err();
        assert!(
            error.contains("profile `missing` not found: available profiles"),
            "{error}"
        );
        assert!(error.contains(": ci"), "{error}");
    }

    #[test]
    fn test_toml_without_profiles_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("kiru.toml");
        std::fs::write(&path, "shell = \"sh\"").unwrap();
        let error = resolve_profile(&path, "ci").unwrap_err();
        assert!(error.contains("failed to parse"), "{error}");
    }

    #[test]
    fn test_resolve_profile_expands_project_dirs() {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("kiru.toml");
        std::fs::write(
            &path,
            r#"
            [profile.ci]
            source = "m.kiru"
            output = "k"

            [profile.ci.project.todo]
            dir = "~/projects/todo"
            "#,
        )
        .unwrap();
        let resolved = resolve_profile(&path, "ci").unwrap();
        assert_eq!(
            resolved.projects["todo"].dir,
            home.join("projects/todo").to_string_lossy()
        );
    }
}
