use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "kiru")]
#[command(about = "kiru is a local project orchestrator CLI", long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    /// Show the resolved configuration (parse, resolve, and validate)
    Status {
        /// Path to kirufile (defaults to ~/.config/kiru/kirufile)
        #[arg(short, long)]
        config: Option<PathBuf>,
    },
    /// Clone/sync project repositories
    Sync {
        /// Path to kirufile (defaults to ~/.config/kiru/kirufile)
        #[arg(short, long)]
        config: Option<PathBuf>,
    },
    /// Run a run block
    Run {
        /// Name of the run block to execute
        name: String,
        /// Path to kirufile (defaults to ~/.config/kiru/kirufile)
        #[arg(short, long)]
        config: Option<PathBuf>,
    },
    /// Compile a `.kiru` DSL file into a `kirufile` s-expression artifact
    Compile {
        /// Path to the `.kiru` source file (defaults to ~/.config/kiru/main.kiru)
        input: Option<PathBuf>,
        /// Output directory for kirufile (defaults to ~/.config/kiru/)
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Run a function directly
    Fn {
        /// Name of the function to run
        name: String,
        /// Project to run the function in
        project: Option<String>,
        /// Path to kirufile (defaults to ~/.config/kiru/kirufile)
        #[arg(short, long)]
        config: Option<PathBuf>,
    },
    /// Print the version number
    Version,
}
