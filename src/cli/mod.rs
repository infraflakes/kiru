mod exec;
mod sync;
mod validate;

use crate::config::{Config, ConfigError, load};
use clap::{Parser, Subcommand};
use std::path::PathBuf;

fn load_config(config_arg: Option<PathBuf>) -> miette::Result<Config> {
    let config_path = get_config_path(config_arg);
    load(&config_path).map_err(|e| {
        if let ConfigError::ParseReports(reports) = e {
            print_parse_errors(reports)
        } else {
            miette::miette!("{}", e)
        }
    })
}

fn load_config_and_resolve(config_arg: Option<PathBuf>) -> miette::Result<Config> {
    let mut config = load_config(config_arg)?;
    crate::config::resolve_uses(&mut config).map_err(|e| miette::miette!("{}", e))?;
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

#[derive(Parser)]
#[command(name = "sro")]
#[command(about = "SRO - Sanctuary Repository Orchestrator", long_about = None)]
struct Cli {
    /// Path to config file
    #[arg(short, long, global = true)]
    config: Option<PathBuf>,

    /// Use plain text output instead of TUI
    #[arg(short, long, global = true)]
    plain: bool,

    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Parse and validate the configuration file
    Validate,
    /// Clone/sync project repositories
    Sync,
    /// Run a sequential execution block
    Seq {
        /// Name of the sequential block to run
        name: String,
        /// Project to run the seq in
        project: String,
    },
    /// Run a parallel execution block
    Par {
        /// Name of the parallel block to run
        name: String,
        /// Project to run the par in
        project: String,
    },
    /// Run a function directly
    Fn {
        /// Name of the function to run
        name: String,
        /// Project to run the function in
        project: String,
    },
    /// Print the version number
    Version,
}

pub fn run() -> miette::Result<()> {
    let cli = Cli::parse();

    match cli.command {
        Commands::Validate => validate::run(cli.config),
        Commands::Sync => sync::run(cli.config, cli.plain),
        Commands::Seq { name, project } => exec::run_seq(cli.config, name, project, cli.plain),
        Commands::Par { name, project } => exec::run_par(cli.config, name, project, cli.plain),
        Commands::Fn { name, project } => exec::run_fn(cli.config, name, project, cli.plain),
        Commands::Version => run_version(),
    }
}

fn get_config_path(config_arg: Option<PathBuf>) -> PathBuf {
    if let Some(path) = config_arg {
        return path;
    }

    if let Some(config_dir) = dirs::config_dir() {
        return config_dir.join("sro").join("config.sro");
    }

    PathBuf::from("config.sro")
}

fn run_version() -> miette::Result<()> {
    println!("sro {}", env!("CARGO_PKG_VERSION"));
    Ok(())
}
