mod args;
mod exec;
mod sync;
mod validate;

pub use args::{Cli, Commands};

use crate::compiler::{CompileError, Sanctuary};
use clap::Parser;
use std::path::PathBuf;

fn load_config(config_arg: Option<PathBuf>) -> miette::Result<Sanctuary> {
    let config_path = get_config_path(config_arg);
    crate::compiler::compile_and_resolve(&config_path).map_err(|e| match e {
        CompileError::ParseReports(reports) => print_parse_errors(reports),
        CompileError::ValidationReport(report) => report,
        _ => miette::miette!("{}", e),
    })
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
    let parsed_cli = Cli::parse();

    match parsed_cli.command {
        Commands::Validate => validate::run(parsed_cli.config),
        Commands::Sync => sync::run(parsed_cli.config),
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
