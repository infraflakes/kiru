use clap::{Command, ValueEnum};
use clap_complete::{Shell, generate_to};
use std::io::Error;
use std::path::PathBuf;

fn main() -> Result<(), Error> {
    let out_dir = PathBuf::from(std::env::var_os("OUT_DIR").expect("OUT_DIR is set by Cargo"));
    std::fs::create_dir_all(&out_dir)?;

    let mut cmd = Command::new("kiru")
        .about("kiru is a local project orchestrator CLI")
        .subcommand_required(true)
        .subcommand(Command::new("validate").about("Validate kiru configuration"))
        .subcommand(Command::new("sync").about("Clone or update project repositories"))
        .subcommand(
            Command::new("run")
                .about("Execute a run block")
                .arg(clap::arg!(<NAME> "Run block name"))
                .arg(clap::arg!(--project <PROJECT> "Project name")),
        )
        .subcommand(
            Command::new("fn")
                .about("Execute a function")
                .arg(clap::arg!(<NAME> "Function name"))
                .arg(clap::arg!(--project <PROJECT> "Project name")),
        )
        .subcommand(Command::new("version").about("Print version information"));

    for &shell in Shell::value_variants() {
        generate_to(shell, &mut cmd, "kiru", &out_dir)?;
    }
    Ok(())
}
