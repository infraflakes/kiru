use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "kiru")]
#[command(about = "kiru is a local project orchestrator CLI", long_about = None)]
pub(crate) struct Cli {
    /// Path to kiru.toml (defaults to ~/.config/kiru/kiru.toml)
    #[arg(short, long, global = true)]
    pub(crate) config: Option<PathBuf>,

    /// Profile to use, declared as [profile.<name>] in kiru.toml
    #[arg(short, long, global = true)]
    pub(crate) profile: Option<String>,

    #[command(subcommand)]
    pub(crate) command: Commands,
}

#[derive(Subcommand)]
pub(crate) enum Commands {
    /// Show current status of kiru
    Status,
    /// Clone/sync project repositories
    Sync,
    /// Run a run block
    Run {
        /// Name of the run block to execute
        name: String,
    },
    /// Compile the profile's source into its output IR file
    Compile,
    /// Print the version number
    Version,
}
