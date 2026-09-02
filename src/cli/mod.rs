mod args;
mod commands;
pub(crate) mod kiru_toml;
mod pager;
mod status;
mod sync;

pub(crate) use args::{Cli, Commands};

use crate::exec::TaskRunError;
use crate::ir::Ir;
use crate::lower::CompileError;
use clap::Parser;
use std::path::PathBuf;

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

/// Map a compile failure. Diagnostics were already printed to stderr by the
/// snippet renderer; I/O failures still need a message.
pub(crate) fn compile_error_to_cli_error(e: CompileError) -> CliError {
    match e {
        CompileError::Io(e) => CliError::Message(format!("I/O error: {}", e)),
        CompileError::Diagnostics(_) => CliError::Reported,
    }
}

/// Load the IR by reading and parsing a `kirufile` (the compiled form of
/// the DSL that `status` and `run` work against).
pub(crate) fn load_config(kirufile_arg: Option<PathBuf>) -> Result<Ir, String> {
    let config_path = get_kirufile_path(kirufile_arg);
    let text = std::fs::read_to_string(&config_path)
        .map_err(|e| format!("failed to read kirufile {}: {}", config_path.display(), e))?;
    Ir::deserialize(&text)
        .map_err(|e| format!("failed to parse kirufile {}: {}", config_path.display(), e))
}

pub(crate) fn run_cli() -> Result<(), CliError> {
    let parsed_cli = Cli::parse();

    match parsed_cli.command {
        Commands::Status { config, kirufile } => status::run_status_command(config, kirufile),
        Commands::Sync { config } => sync::run_sync_command(config),
        Commands::Run {
            name,
            config,
            kirufile,
        } => commands::execute_run_block(config, kirufile, name),
        Commands::Compile { config, output } => compile::run_compile_command(config, output),
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

/// Resolve the `kiru.toml` path from `-c`, falling back to the canonical
/// `~/.config/kiru/kiru.toml`.
pub(crate) fn get_toml_path(config_arg: Option<PathBuf>) -> PathBuf {
    config_arg.unwrap_or_else(kiru_toml::get_kiru_toml_path)
}

/// Resolve the kirufile path from `-p`, falling back to the canonical
/// `~/.config/kiru/kirufile`.
fn get_kirufile_path(kirufile_arg: Option<PathBuf>) -> PathBuf {
    kirufile_arg.unwrap_or_else(|| kiru_config_dir().join("kirufile"))
}
