//! The compile pipeline, in order.
//!
//! This is the one place the engine's stages are named in sequence: load the
//! files, resolve names into edges, validate types and flow, prune what the
//! entry cannot reach, lower the result to bytecode, then copy this executable
//! and attach the serialized bytecode behind the binary trailer. The command
//! line layer only parses arguments and prints the message this returns.

use std::path::Path;

use crate::compiler::{
    DIAGNOSTIC_PREFIX, attach_trailer, load_files, lower_bytecode, prune_unreachable,
    resolve_names, serialize_bytecode, validate_program,
};

use super::diagnostic::render;
use super::output_file::{make_writable_executable, same_file};

/// Compile `entry` to `output`, or to the entry path without its extension,
/// by copying `executable` (the running `kc`) and attaching the program. On
/// failure, returns the message to print, already rendered.
pub(crate) fn compile(
    entry: &Path,
    output: Option<&Path>,
    executable: &Path,
) -> Result<(), String> {
    let loaded = load_files(entry).map_err(|error| error.render())?;
    let mut program = resolve_names(&loaded).map_err(|diagnostic| render(&loaded, &diagnostic))?;
    validate_program(&mut program).map_err(|diagnostic| render(&loaded, &diagnostic))?;
    prune_unreachable(&mut program);
    let bytecode = lower_bytecode(&program);

    let bytes = serialize_bytecode(&bytecode)
        .map_err(|error| format!("{DIAGNOSTIC_PREFIX}: cannot serialize the program: {error}\n"))?;
    let output_path = output
        .map(Path::to_path_buf)
        .unwrap_or_else(|| entry.with_extension(""));
    if same_file(&loaded.files[loaded.entry].path, &output_path) {
        return Err(format!(
            "{DIAGNOSTIC_PREFIX}: the output path would overwrite the entry file\n"
        ));
    }
    std::fs::copy(executable, &output_path).map_err(|error| {
        format!(
            "{DIAGNOSTIC_PREFIX}: cannot write {}: {error}\n",
            output_path.display()
        )
    })?;
    // A copy keeps the source's mode, and an installed `kc` may be read-only
    // (a package manager or a Nix store path), so make the output a writable
    // executable before the trailer is appended to it.
    make_writable_executable(&output_path).map_err(|error| {
        format!(
            "{DIAGNOSTIC_PREFIX}: cannot write {}: {error}\n",
            output_path.display()
        )
    })?;
    attach_trailer(&output_path, &bytes).map_err(|error| {
        format!(
            "{DIAGNOSTIC_PREFIX}: cannot write {}: {error}\n",
            output_path.display()
        )
    })?;
    Ok(())
}
