use clap::{CommandFactory, ValueEnum};
use clap_complete::{Shell, generate_to};
use std::io::Error;
use std::path::PathBuf;

// The CLI definition lives only in `src/cli/args.rs`. Reusing the derive here
// (instead of hand-mirroring the command in this file) keeps the completions
// generated from the very same definition the binary parses with, so the two
// can never drift out of sync.
mod args {
    include!("src/cli/args.rs");
}

fn main() -> Result<(), Error> {
    let out_dir = PathBuf::from(
        std::env::var_os("KIRU_COMPLETIONS_DIR")
            .or_else(|| std::env::var_os("OUT_DIR"))
            .expect("OUT_DIR or KIRU_COMPLETIONS_DIR must be set"),
    );
    std::fs::create_dir_all(&out_dir)?;

    let mut cmd = args::Cli::command();

    for &shell in Shell::value_variants() {
        generate_to(shell, &mut cmd, "kiru", &out_dir)?;
    }
    Ok(())
}
