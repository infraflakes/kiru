use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "kiru")]
#[command(about = "kiru is a local project orchestrator CLI", long_about = None)]
pub struct Cli {
    /// Path to config file
    #[arg(short, long, global = true)]
    pub config: Option<PathBuf>,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
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
