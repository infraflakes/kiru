mod args;
pub(crate) mod kiru_toml;
mod pager;
mod run;
mod status;
mod sync;

use args::{Cli, Commands};

use crate::compile::CompileError;
use crate::exec::{ProjectExec, ProjectSync, TaskRunError};
use crate::ir::Program;
use clap::Parser;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

pub(crate) mod compile;

/// Terminal outcome of a CLI command. The distinction decides whether
/// `main` still has something to print.
pub(crate) enum CliError {
    /// The failure has not been shown to the user yet; print the message.
    Message(String),
    /// The failure was already rendered to the user (compile diagnostics via
    /// the snippet renderer, task outcomes via the TUI); only the non-zero
    /// exit code remains.
    Reported,
}

impl CliError {
    /// Wrap a not-yet-shown message.
    pub(crate) fn message(msg: impl Into<String>) -> Self {
        CliError::Message(msg.into())
    }
}

impl From<TaskRunError> for CliError {
    fn from(error: TaskRunError) -> Self {
        match error {
            TaskRunError::TaskFailed => CliError::Reported,
            TaskRunError::Infrastructure(message) => CliError::Message(message),
        }
    }
}

/// Map a compile failure: diagnostics are printed to stderr by the snippet
/// renderer here, so only the exit code remains (`Reported`); I/O failures
/// still need a message.
pub(crate) fn compile_error_to_cli_error(e: CompileError) -> CliError {
    match e {
        CompileError::Io(e) => CliError::Message(format!("I/O error: {}", e)),
        CompileError::Diagnostics(diags) => {
            for d in &diags {
                crate::diagnostics::print_diagnostic(d);
            }
            CliError::Reported
        }
    }
}

/// Why the compiled program could not be loaded. The absent case is not an
/// error for every command: `status` reports what is configured without one,
/// while `run` requires it.
pub(crate) enum ProgramLoadError {
    /// The file exists but is not a valid program.
    Invalid { path: PathBuf, message: String },
}

/// Load the compiled program from the profile's `output`. `Ok(None)` means
/// nothing has been compiled yet.
pub(crate) fn load_program(path: &Path) -> Result<Option<Program>, ProgramLoadError> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => {
            return Err(ProgramLoadError::Invalid {
                path: path.to_path_buf(),
                message: format!("failed to read: {e}"),
            });
        }
    };
    match Program::deserialize(&text) {
        Ok(program) => Ok(Some(program)),
        Err(message) => Err(ProgramLoadError::Invalid {
            path: path.to_path_buf(),
            message,
        }),
    }
}

impl ProgramLoadError {
    /// The user-facing message of a malformed compiled program.
    pub(crate) fn message(&self) -> String {
        match self {
            ProgramLoadError::Invalid { path, message } => {
                format!(
                    "failed to parse compiled program {}: {message}",
                    path.display()
                )
            }
        }
    }
}

/// The project map `run` executes against: every project with a directory.
pub(crate) fn exec_projects(
    profile: &kiru_toml::ResolvedProfile,
) -> Arc<BTreeMap<String, ProjectExec>> {
    let mut projects = BTreeMap::new();
    for (name, project) in &profile.projects {
        if !project.dir.is_empty() {
            projects.insert(
                name.clone(),
                ProjectExec {
                    dir: PathBuf::from(&project.dir),
                    direnv: project.direnv,
                },
            );
        }
    }
    Arc::new(projects)
}

/// The project list `sync` works through: only projects with both a url and
/// a directory are syncable; the rest are reported and skipped.
pub(crate) fn sync_projects(profile: &kiru_toml::ResolvedProfile) -> Vec<ProjectSync> {
    profile
        .projects
        .iter()
        .filter_map(|(name, project)| {
            let skip_reason = if project.url.is_empty() && project.dir.is_empty() {
                Some("missing url and dir")
            } else if project.url.is_empty() {
                Some("missing url")
            } else if project.dir.is_empty() {
                Some("missing dir")
            } else {
                None
            };
            if let Some(reason) = skip_reason {
                eprintln!("Warning: project {name:?}: {}, skipping sync", reason);
                None
            } else {
                Some(ProjectSync {
                    name: name.clone(),
                    url: project.url.clone(),
                    dir: project.dir.clone(),
                    branch: project.branch.clone(),
                })
            }
        })
        .collect()
}

/// The shell of a profile, or the POSIX default.
pub(crate) fn profile_shell(profile: &kiru_toml::ResolvedProfile) -> String {
    profile.shell.clone().unwrap_or_else(|| "sh".to_string())
}

/// The command timeout of a profile.
pub(crate) fn profile_timeout(profile: &kiru_toml::ResolvedProfile) -> Option<Duration> {
    profile.timeout.map(Duration::from_secs)
}

/// Resolve the `kiru.toml` path from `-c`, falling back to the canonical
/// `~/.config/kiru/kiru.toml`.
pub(crate) fn get_toml_path(config_arg: Option<PathBuf>) -> PathBuf {
    config_arg.unwrap_or_else(kiru_toml::get_kiru_toml_path)
}

/// Resolve the `-p` profile selection into a [`ResolvedProfile`]. Every
/// command except `version` requires a profile; the error names the flag
/// so the fix is obvious.
pub(crate) fn resolve_selected_profile(
    config_arg: Option<PathBuf>,
    profile_arg: Option<&str>,
) -> Result<(kiru_toml::ResolvedProfile, PathBuf), CliError> {
    let profile_name = profile_arg
        .ok_or_else(|| CliError::message("a profile is required: pass -p/--profile <name>"))?;
    let toml_path = get_toml_path(config_arg);
    let profile =
        kiru_toml::resolve_profile(&toml_path, profile_name).map_err(CliError::message)?;
    Ok((profile, toml_path))
}

pub(crate) fn run_cli() -> Result<(), CliError> {
    let parsed_cli = Cli::parse();
    let profile_arg = parsed_cli.profile.as_deref();

    match parsed_cli.command {
        Commands::Status => status::run_status_command(parsed_cli.config, profile_arg),
        Commands::Sync => sync::run_sync_command(parsed_cli.config, profile_arg),
        Commands::Run { name } => run::execute_run_block(parsed_cli.config, profile_arg, name),
        Commands::Compile => compile::run_compile_command(parsed_cli.config, profile_arg),
        Commands::Version => {
            println!("kiru {}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
    }
}

/// Default configuration directory: `~/.config/kiru/`.
pub(crate) fn kiru_config_dir() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".config")
        .join("kiru")
}
