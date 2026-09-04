//! `kiru compile` command: parses a `.kiru` source file and writes the
//! compiled `kirufile` that the other commands read.

use std::path::PathBuf;

use super::CliError;

pub(crate) fn run_compile_command(
    config_arg: Option<PathBuf>,
    output: Option<PathBuf>,
) -> Result<(), CliError> {
    let input = config_arg.unwrap_or_else(|| super::kiru_config_dir().join("main.kiru"));
    let output = output
        .map(|dir| dir.join("kirufile"))
        .unwrap_or_else(|| super::kiru_config_dir().join("kirufile"));

    let ir = crate::compile::compile_path(&input).map_err(super::compile_error_to_cli_error)?;

    let text = ir.serialize();
    std::fs::write(&output, text).map_err(|e| {
        CliError::message(format!(
            "failed to write kirufile {}: {}",
            output.display(),
            e
        ))
    })?;

    println!("compiled {} -> {}", input.display(), output.display());
    Ok(())
}
