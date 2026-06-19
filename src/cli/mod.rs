mod args;
mod exec;
mod sync;
mod validate;

pub use args::{Cli, Commands};

use crate::config::{Config, ConfigError, load};
use clap::Parser;
use std::path::PathBuf;

fn load_config(config_arg: Option<PathBuf>) -> miette::Result<Config> {
    let config_path = get_config_path(config_arg);
    load(&config_path).map_err(|e| match e {
        ConfigError::ParseReports(reports) => print_parse_errors(reports),
        ConfigError::ValidationReport(report) => report,
        _ => miette::miette!("{}", e),
    })
}

fn load_config_and_resolve(config_arg: Option<PathBuf>) -> miette::Result<Config> {
    let mut config = load_config(config_arg)?;
    match crate::config::resolve_includes(&mut config) {
        Ok(()) => {}
        Err(crate::config::ConfigError::ParseReports(reports)) => {
            return Err(print_parse_errors(reports));
        }
        Err(crate::config::ConfigError::ValidationReport(report)) => {
            return Err(report);
        }
        Err(e) => {
            return Err(miette::miette!("{}", e));
        }
    }
    crate::config::validate(&config).map_err(|e| match e {
        crate::config::ConfigError::ValidationReport(report) => report,
        _ => miette::miette!("{}", e),
    })?;
    Ok(config)
}

fn print_parse_errors(reports: Vec<miette::Report>) -> miette::Report {
    let count = reports.len();
    if count == 1 {
        reports.into_iter().next().unwrap()
    } else {
        let mut combined = String::new();
        for (i, report) in reports.into_iter().enumerate() {
            if i > 0 {
                combined.push('\n');
            }
            combined.push_str(&format!("{:?}", report));
        }
        miette::miette!("{}\n{} parse error(s) found", combined, count)
    }
}

pub fn run() -> miette::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Validate => validate::run(cli.config),
        Commands::Sync => sync::run(cli.config),
        Commands::Run { name, project } => exec::run_run(cli.config, name, project),
        Commands::Fn { name, project } => exec::run_fn(cli.config, name, project),
        Commands::Version => run_version(),
    }
}

fn get_config_path(config_arg: Option<PathBuf>) -> PathBuf {
    if let Some(path) = config_arg {
        return path;
    }

    crate::config::default_config_path()
}

fn run_version() -> miette::Result<()> {
    println!("kiru {}", env!("CARGO_PKG_VERSION"));
    Ok(())
}
