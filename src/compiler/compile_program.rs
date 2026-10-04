//! The compile pipeline, in order.
//!
//! This is the one place the engine's stages are named in sequence: load the
//! files, resolve names into edges, validate kinds and flow, prune what the
//! entry cannot reach, then copy this executable and attach the serialized
//! program behind the binary trailer. The command line layer only parses
//! arguments and prints the message this returns.

use std::path::Path;

use crate::compiler::{
    DIAGNOSTIC_PREFIX, Diagnostic, LoadedProgram, attach_trailer, load_files, prune_unreachable,
    resolve_names, serialize_program, validate_program,
};

/// Compile `entry` to `output`, or to the entry path without its extension,
/// by copying `executable` (the running `kc`) and attaching the program. On
/// failure, returns the message to print, already rendered.
pub(crate) fn compile(
    entry: &Path,
    output: Option<&Path>,
    executable: &Path,
) -> Result<(), String> {
    let mut loaded = load_files(entry).map_err(|error| error.render())?;
    let mut program =
        resolve_names(&mut loaded).map_err(|diagnostic| render(&loaded, &diagnostic))?;
    validate_program(&mut program).map_err(|diagnostic| render(&loaded, &diagnostic))?;
    prune_unreachable(&mut program);

    let bytes = serialize_program(&program)
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
    attach_trailer(&output_path, &bytes).map_err(|error| {
        format!(
            "{DIAGNOSTIC_PREFIX}: cannot write {}: {error}\n",
            output_path.display()
        )
    })?;
    Ok(())
}

/// Whether `output` names the same file as `entry`, following symlinks and
/// `..` when the output already exists.
fn same_file(entry: &Path, output: &Path) -> bool {
    let entry = entry.canonicalize().unwrap_or_else(|_| entry.to_path_buf());
    let output = match output.canonicalize() {
        Ok(canonical) => canonical,
        Err(_) => match (output.parent(), output.file_name()) {
            (Some(parent), Some(name)) => parent
                .canonicalize()
                .map(|parent| parent.join(name))
                .unwrap_or_else(|_| output.to_path_buf()),
            _ => output.to_path_buf(),
        },
    };
    entry == output
}

/// Render a diagnostic against the source of the file it points at.
fn render(program: &LoadedProgram, diagnostic: &Diagnostic) -> String {
    let source = program
        .files
        .iter()
        .find(|file| file.path == diagnostic.path)
        .map(|file| file.source.as_str())
        .unwrap_or("");
    diagnostic.render(source)
}
