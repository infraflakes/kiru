//! `kiru compile` command: compiles the selected profile's `source` into
//! the profile's `output` IR file, creating the output's parent
//! directories when missing.

use super::CliError;

pub(crate) fn run_compile_command(
    config_arg: Option<std::path::PathBuf>,
    profile_arg: Option<&str>,
) -> Result<(), CliError> {
    let (profile, _) = super::resolve_selected_profile(config_arg, profile_arg)?;

    let ir =
        crate::compile::compile_path(&profile.source).map_err(super::compile_error_to_cli_error)?;

    // The output may sit in a directory that does not exist yet (a nested
    // path in the profile): create the parents so the write cannot fail on
    // missing structure.
    if let Some(parent) = profile.output.parent() {
        std::fs::create_dir_all(parent).map_err(|e| {
            CliError::message(format!(
                "failed to create output directory {}: {}",
                parent.display(),
                e
            ))
        })?;
    }

    let text = ir.serialize();
    std::fs::write(&profile.output, text).map_err(|e| {
        CliError::message(format!(
            "failed to write compiled program {}: {}",
            profile.output.display(),
            e
        ))
    })?;

    println!(
        "compiled {} -> {}",
        profile.source.display(),
        profile.output.display()
    );
    Ok(())
}
