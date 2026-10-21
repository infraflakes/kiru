//! The compile process: a named entry file becomes a checked program, and the
//! checked program becomes an executable.
//!
//! The stages run in order: `build` loads the files, resolves every name into
//! an edge, selects the entry, and checks every body; then the reachable set
//! is computed, what the entry never reaches is pruned, the result is lowered
//! to bytecode, and the image is packaged behind the binary trailer. A failed
//! build reports everything it could still earn and writes no output.

use std::path::Path;

use super::warnings::{is_program_file, unused_declarations};
use crate::compiler::diagnostic::render;
use crate::compiler::{
    Build, DIAGNOSTIC_PREFIX, ProgramSource, attach_trailer, build, lower_bytecode, mark_reachable,
    prune_unreachable, serialize_bytecode,
};
use package::{make_writable_executable, same_file};

pub(crate) mod build;
pub(crate) mod check;
pub(crate) mod load;
pub(crate) mod lower;
pub(crate) mod package;
pub(crate) mod prune;
pub(crate) mod resolve;

#[cfg(test)]
mod tests;

/// Compile `entry` to `output`, or to the entry path without its extension,
/// by copying `executable` (the running `kc`) and attaching the program.
pub(crate) fn compile(
    entry: &Path,
    output: Option<&Path>,
    executable: &Path,
    source: &impl ProgramSource,
) -> Result<(), String> {
    let Build {
        loaded,
        mut program,
        entry: entry_id,
        warnings,
        syntax,
    } = build(entry, source).map_err(|failure| failure.rendered().to_owned())?;
    // The reachable set is the compile's second reading of the graph: what no
    // reachable body reads is a warning, then it is dropped before lowering.
    let reachable = mark_reachable(&program, entry_id);
    let mut facts = warnings;
    facts.extend(unused_declarations(&program, &reachable));
    // The program's facts alone: the library the compiler carries is not the
    // program and never warns for it.
    facts.retain(|warning| is_program_file(&warning.path));
    // A recovered tree cannot become a binary, but it still reports: every
    // syntax error it kept, and every fact the recovered source earned.
    if !syntax.is_empty() {
        let mut message = String::new();
        for diagnostic in syntax.iter().chain(facts.iter()) {
            message.push_str(&render(&loaded, diagnostic));
        }
        return Err(message);
    }
    for warning in &facts {
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
