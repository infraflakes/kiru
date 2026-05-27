use clap::{CommandFactory, ValueEnum};
use clap_complete::{Shell, generate_to};
use std::io::Error;

include!("src/cli/args.rs");

fn main() -> Result<(), Error> {
    let out_dir = "completions";
    std::fs::create_dir_all(out_dir)?;

    let mut cmd = Cli::command();
    for &shell in Shell::value_variants() {
        generate_to(shell, &mut cmd, "kiru", out_dir)?;
    }
    Ok(())
}
