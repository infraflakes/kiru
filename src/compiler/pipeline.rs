//! The compile pipeline, in order.
//!
//! This is the one place the engine's stages are named in sequence: the build
//! loads the files, resolves names into edges, selects the entry, and validates
//! types and flow; its warnings print to the terminal; then mark what the
//! entry reaches, prune the rest, lower the result to bytecode, and copy this
//! executable and attach the serialized bytecode behind the binary trailer. The
//! command line layer only parses arguments and prints the message this
//! returns.

use std::path::Path;

use super::output_file::{make_writable_executable, same_file};
use crate::compiler::diagnostic::render;
use crate::compiler::sources::DiskSource;
use crate::compiler::{
    Build, DIAGNOSTIC_PREFIX, attach_trailer, build, lower_bytecode, mark_reachable,
    prune_unreachable, serialize_bytecode, unused_declarations,
};

/// Compile `entry` to `output`, or to the entry path without its extension,
/// by copying `executable` (the running `kc`) and attaching the program. On
/// failure, returns the message to print, already rendered.
pub(crate) fn compile(
    entry: &Path,
    output: Option<&Path>,
    executable: &Path,
) -> Result<(), String> {
    let Build {
        loaded,
        mut program,
        entry: entry_id,
        warnings,
    } = build(entry, &DiskSource).map_err(|failure| failure.rendered().to_owned())?;
    // The reachable set is the compile's second reading of the graph: what no
    // reachable body reads is a warning, then it is dropped before lowering.
    let reachable = mark_reachable(&program, entry_id);
    let mut warnings = warnings;
    warnings.extend(unused_declarations(&program, &reachable));
    for warning in &warnings {
        eprint!("{}", render(&loaded, warning));
    }
    let entry_id = prune_unreachable(&mut program, entry_id, &reachable);
    let bytecode = lower_bytecode(&program, entry_id);

    let bytes = serialize_bytecode(&bytecode)
        .map_err(|error| format!("{DIAGNOSTIC_PREFIX}: cannot serialize the program: {error}\n"))?;
    let entry_file = loaded.entry.expect("a compilation names its entry file");
    let output_path = output
        .map(Path::to_path_buf)
        .unwrap_or_else(|| entry.with_extension(""));
    if same_file(&loaded.files[entry_file].path, &output_path) {
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
