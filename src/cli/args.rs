use clap::{Parser, Subcommand};
use std::path::PathBuf;

#[derive(Parser)]
#[command(name = "kiru")]
#[command(about = "kiru is a local project orchestrator CLI", long_about = None)]
pub(crate) struct Cli {
    #[command(subcommand)]
    pub(crate) command: Commands,
}

#[derive(Subcommand)]
pub(crate) enum Commands {
    /// Show the resolved configuration (parse, resolve, and validate)
    Status {
        /// Path to kiru.toml (defaults to ~/.config/kiru/kiru.toml)
        #[arg(short, long)]
        config: Option<PathBuf>,
        /// Path to kirufile (defaults to ~/.config/kiru/kirufile)
        #[arg(short = 'p', long)]
        kirufile: Option<PathBuf>,
    },
    /// Clone/sync project repositories
    Sync {
        /// Path to kiru.toml (defaults to ~/.config/kiru/kiru.toml)
        #[arg(short, long)]
        config: Option<PathBuf>,
    },
    /// Run a run block
    Run {
        /// Name of the run block to execute
        name: String,
        /// Path to kiru.toml (defaults to ~/.config/kiru/kiru.toml)
        #[arg(short, long)]
        config: Option<PathBuf>,
        /// Path to kirufile (defaults to ~/.config/kiru/kirufile)
        #[arg(short = 'p', long)]
        kirufile: Option<PathBuf>,
    },
    /// Compile a `.kiru` DSL file into a `kirufile`
    Compile {
        /// Path to the `.kiru` source file (defaults to ~/.config/kiru/main.kiru)
        #[arg(short, long)]
        config: Option<PathBuf>,
        /// Output directory for kirufile (defaults to ~/.config/kiru/)
        #[arg(short, long)]
        output: Option<PathBuf>,
    },
    /// Print the version number
    Version,
}
