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
    /// Show the resolved configuration (parse, resolve, and validate)
    Status,
    /// Clone/sync project repositories
    Sync,
    /// Run a run block
    Run {
        /// Name of the run block to execute
        name: String,
    },
    /// Compile a `.kiru` DSL file into a `kirufile` s-expression artifact
    Compile {
        /// Path to the `.kiru` source file (defaults to the `-c` value or
        /// `main.kiru` in the current directory)
        input: Option<PathBuf>,
        /// Output `kirufile` path (defaults to `kirufile` in the current dir)
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Run a function directly
    Fn {
        /// Name of the function to run
        name: String,
        /// Project to run the function in
        project: Option<String>,
    },
    /// Print the version number
    Version,
}
