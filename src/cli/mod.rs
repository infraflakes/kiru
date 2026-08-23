mod args;
mod exec;
mod pager;
mod status;
mod sync;

pub use args::{Cli, Commands};

use crate::compiler::CompileError;
use crate::plan::Plan;
use clap::Parser;
use std::path::{Path, PathBuf};

/// Load a plan through the given compiler entry point (full compile or
/// metadata-only for sync), mapping compiler errors to miette reports once.
fn load_config_via(
    config_arg: Option<PathBuf>,
    load: impl FnOnce(&Path) -> Result<Plan, CompileError>,
) -> miette::Result<Plan> {
    let config_path = get_config_path(config_arg);
    load(&config_path).map_err(compile_error_to_report)
}

fn load_config(config_arg: Option<PathBuf>) -> miette::Result<Plan> {
    let force_cwd = crate::runner::kiru_cwd_enabled();
    load_config_via(config_arg, |config_path| {
        crate::compiler::compile_and_resolve(config_path, force_cwd)
    })
}

/// Map a compiler error to a miette report for the CLI. Single owner of the
/// `CompileError` → diagnostic mapping so adding a variant updates every
/// command path (status, sync, run, fn) at once instead of drifting.
pub(crate) fn compile_error_to_report(e: CompileError) -> miette::Report {
    match e {
        CompileError::ParseReports(reports) => batch_report(reports, "parse"),
        CompileError::ValidationReport(reports) => batch_report(reports, "validation"),
        _ => miette::miette!("{}", e),
    }
}

/// Print every diagnostic in a batch and return the batch summary error.
/// Shared by the parse and validation error paths so the print-then-summarize
/// shape exists in exactly one place.
fn batch_report(reports: Vec<miette::Report>, what: &str) -> miette::Report {
    let count = reports.len();
    for report in &reports {
        crate::error::print_diagnostic(report);
    }
    miette::miette!("{} {} error(s) found", count, what)
}

pub fn run_cli() -> miette::Result<()> {
    let parsed_cli = Cli::parse();

    match parsed_cli.command {
        Commands::Status => status::run_status_command(parsed_cli.config),
        Commands::Sync => sync::run_sync_command(parsed_cli.config),
        Commands::Run { name } => exec::execute_run_block(parsed_cli.config, name),
        Commands::Fn { name, project } => exec::execute_function(parsed_cli.config, name, project),
        Commands::Version => run_version(),
    }
}

fn get_config_path(config_arg: Option<PathBuf>) -> PathBuf {
    if let Some(path) = config_arg {
        return path;
    }

    // In CI/CD (or any invocation where `KIRU_CWD=1` is set) the caller is
    // already inside the project, so resolve the config to `main.kiru` in the
    // current directory rather than the global `~/.config/kiru/main.kiru`.
    if crate::runner::kiru_cwd_enabled() {
        return PathBuf::from("main.kiru");
    }

    if let Some(config_dir) = dirs::config_dir() {
        return config_dir.join("kiru").join("main.kiru");
    }
    PathBuf::from("main.kiru")
}

fn run_version() -> miette::Result<()> {
    println!("kiru {}", env!("CARGO_PKG_VERSION"));
    Ok(())
}
