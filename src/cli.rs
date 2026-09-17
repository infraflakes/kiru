mod args;
mod commands;
pub(crate) mod kiru_toml;
mod pager;
mod status;
mod sync;

use args::{Cli, Commands};

use crate::compile::CompileError;
use crate::exec::TaskRunError;
use crate::ir::Ir;
use clap::Parser;
use std::path::{Path, PathBuf};

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

/// Load the IR by reading and parsing the compiled kirufile (the compiled
/// form of the DSL that `status` and `run` work against). The path comes
/// from the profile's `output`.
pub(crate) fn load_config(ir_path: &Path) -> Result<Ir, String> {
    let text = std::fs::read_to_string(ir_path)
        .map_err(|e| format!("failed to read kirufile {}: {}", ir_path.display(), e))?;
    Ir::deserialize(&text)
        .map_err(|e| format!("failed to parse kirufile {}: {}", ir_path.display(), e))
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
        Commands::Run { name } => {
            commands::run::execute_run_block(parsed_cli.config, profile_arg, name)
        }
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
