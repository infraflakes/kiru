//! `kiru compile` command: parses a `.kiru` source file and writes the
//! compiled `kirufile` that the other commands read.

use std::path::PathBuf;

use super::CliError;

fn resolve_compile_input(config_arg: Option<PathBuf>) -> PathBuf {
    if let Some(path) = config_arg {
        return path;
    }
    super::kiru_config_dir().join("main.kiru")
}

fn resolve_compile_output(output: Option<PathBuf>) -> PathBuf {
    if let Some(dir) = output {
        return dir.join("kirufile");
    }
    super::kiru_config_dir().join("kirufile")
}

pub(crate) fn run_compile_command(
    config_arg: Option<PathBuf>,
    output: Option<PathBuf>,
) -> Result<(), CliError> {
    let input = resolve_compile_input(config_arg);
    let output = resolve_compile_output(output);

    let ir = crate::lower::lower_and_resolve(&input).map_err(super::compile_error_to_cli_error)?;

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
