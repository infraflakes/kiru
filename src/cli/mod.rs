mod args;
mod commands;
pub(crate) mod kiru_toml;
mod pager;
mod status;
mod sync;

pub(crate) use args::{Cli, Commands};

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

/// Load the IR by reading and parsing a `kirufile` (the compiled form of
/// the DSL that `status` and `run` work against).
pub(crate) fn load_config(kirufile_arg: Option<PathBuf>) -> Result<Ir, String> {
    let config_path = get_kirufile_path(kirufile_arg);
    let text = std::fs::read_to_string(&config_path)
        .map_err(|e| format!("failed to read kirufile {}: {}", config_path.display(), e))?;
    Ir::deserialize(&text)
        .map_err(|e| format!("failed to parse kirufile {}: {}", config_path.display(), e))
}

pub(crate) fn run_cli() -> Result<(), String> {
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
