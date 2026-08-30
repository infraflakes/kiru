mod args;
mod commands;
mod pager;
mod status;
mod sync;

pub use args::{Cli, Commands};

use crate::ir::Ir;
use crate::lower::CompileError;
use clap::Parser;
use std::path::PathBuf;

pub(crate) mod compile;

/// Print compile errors to stderr (snippets rendered via annotate-snippets)
/// and return empty string to signal "already printed". Non-empty return is
/// only for non-diagnostic errors (I/O) where main.rs should add prefix.
pub(crate) fn compile_error_to_string(e: CompileError) -> String {
    match e {
        CompileError::Io(e) => format!("I/O error: {}", e),
        CompileError::Parse(diags) | CompileError::Validation(diags) => {
            for d in &diags {
                crate::diagnostics::print_diagnostic(d);
            }
            String::new() // already printed
        }
    }
}

/// Load an IR by reading and parsing a `kirufile` artifact directly.
pub(crate) fn load_config(config_arg: Option<PathBuf>) -> Result<Ir, String> {
    let config_path = get_config_path(config_arg);
    let text = std::fs::read_to_string(&config_path)
        .map_err(|e| format!("failed to read kirufile {}: {}", config_path.display(), e))?;
    Ir::deserialize(&text)
        .map_err(|e| format!("failed to parse kirufile {}: {}", config_path.display(), e))
}

pub fn run_cli() -> Result<(), String> {
    let parsed_cli = Cli::parse();

    match parsed_cli.command {
        Commands::Status { config } => status::run_status_command(config),
        Commands::Sync { config } => sync::run_sync_command(config),
        Commands::Run { name, config } => commands::execute_run_block(config, name),
        Commands::Fn {
            name,
            project,
            config,
        } => commands::execute_function(config, name, project),
        Commands::Compile { input, output } => compile::run_compile_command(input, output),
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

/// Resolve the path to the kirufile artifact.
fn get_config_path(config_arg: Option<PathBuf>) -> PathBuf {
    if let Some(path) = config_arg {
        return path;
    }
    if crate::exec::kiru_cwd_enabled() {
        return PathBuf::from("kirufile");
    }
    kiru_config_dir().join("kirufile")
}
