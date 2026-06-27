mod args;
mod exec;
mod sync;
mod validate;

pub use args::{Cli, Commands};

use crate::compiler::{CompileError, Sanctuary, compile};
use clap::Parser;
use std::path::PathBuf;

fn load_config(config_arg: Option<PathBuf>) -> miette::Result<Sanctuary> {
    let config_path = get_config_path(config_arg);
    compile(&config_path).map_err(|e| match e {
        CompileError::ParseReports(reports) => print_parse_errors(reports),
        CompileError::ValidationReport(report) => report,
        _ => miette::miette!("{}", e),
    })
}

fn load_config_and_resolve(config_arg: Option<PathBuf>) -> miette::Result<Sanctuary> {
    let mut config = load_config(config_arg)?;
    match crate::compiler::resolve_includes(&mut config) {
        Ok(()) => {}
        Err(crate::compiler::CompileError::ParseReports(reports)) => {
            return Err(print_parse_errors(reports));
        }
        Err(crate::compiler::CompileError::ValidationReport(report)) => {
            return Err(report);
        }
        Err(e) => {
            return Err(miette::miette!("{}", e));
        }
    }
    crate::compiler::validate(&config).map_err(|e| match e {
        crate::compiler::CompileError::ValidationReport(report) => report,
        _ => miette::miette!("{}", e),
    })?;
    Ok(config)
}

fn print_parse_errors(reports: Vec<miette::Report>) -> miette::Report {
    let count = reports.len();
    let mut combined = String::new();
    for (i, report) in reports.into_iter().enumerate() {
        if i > 0 {
            combined.push('\n');
        }
        combined.push_str(&format!("{:?}", report));
    }
    miette::miette!("{}\n{} parse error(s) found", combined, count)
}

pub fn run() -> miette::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Validate => validate::run(cli.config),
        Commands::Sync => sync::run(cli.config),
        Commands::Run { name, project } => exec::execute_run_block(cli.config, name, project),
        Commands::Fn { name, project } => exec::execute_function(cli.config, name, project),
        Commands::Version => run_version(),
    }
}

fn get_config_path(config_arg: Option<PathBuf>) -> PathBuf {
    if let Some(path) = config_arg {
        return path;
    }

    if crate::compiler::is_sanctuary_disabled() {
        return PathBuf::from(".kiru").join("main.kiru");
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
