mod args;
mod exec;
mod pager;
mod status;
mod sync;

pub use args::{Cli, Commands};

use crate::compiler::{CompileError, Config};
use clap::Parser;
use std::path::PathBuf;

fn load_config(config_arg: Option<PathBuf>) -> miette::Result<Config> {
    let config_path = get_config_path(config_arg);
    crate::compiler::compile_and_resolve(&config_path).map_err(|e| match e {
        CompileError::ParseReports(reports) => print_parse_errors(reports),
        CompileError::ValidationReport(report) => report,
        _ => miette::miette!("{}", e),
    })
}

fn print_parse_errors(reports: Vec<miette::Report>) -> miette::Report {
    let count = reports.len();
    for report in &reports {
        print_diagnostic(report);
    }
    miette::miette!("{} parse error(s) found", count)
}

/// Render a miette diagnostic to stderr using the installed handler.
///
/// Centralizes diagnostic printing so callers do not reach for ad-hoc
/// `eprintln!("{:?}", report)`, which drops the handler's source snippets and
/// styling. The handler is installed once in `main` via `miette::set_hook`.
pub(crate) fn print_diagnostic(report: &miette::Report) {
    use std::io::Write;

    let mut stderr = std::io::stderr();
    if writeln!(stderr, "{:?}", report).is_err() {
        std::eprintln!("{:?}", report);
    }
}

pub fn run_cli() -> miette::Result<()> {
    let parsed_cli = Cli::parse();

    match parsed_cli.command {
        Commands::Status => status::run_status_command(parsed_cli.config),
        Commands::Sync => sync::run_sync_command(parsed_cli.config),
        Commands::Run { name, project } => {
            exec::execute_run_block(parsed_cli.config, name, project)
        }
        Commands::Fn { name, project } => exec::execute_function(parsed_cli.config, name, project),
        Commands::Version => run_version(),
    }
}

fn get_config_path(config_arg: Option<PathBuf>) -> PathBuf {
    if let Some(path) = config_arg {
        return path;
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
