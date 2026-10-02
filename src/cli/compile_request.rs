//! Compiling an entry file into a standalone executable.

use std::path::Path;

use crate::compiler::{Diagnostic, LoadedProgram, analyze, append, capture, link, load, retain};

/// Load, link, check, and retain the entry program, then copy this executable
/// and append the payload behind a trailer. Returns the process exit code.
pub(crate) fn compile(entry: &Path, output: Option<&Path>) -> i32 {
    let mut loaded = match load(entry) {
        Ok(loaded) => loaded,
        Err(error) => {
            eprint!("{}", error.render());
            return 1;
        }
    };
    let mut program = match link(&mut loaded) {
        Ok(program) => program,
        Err(diagnostic) => {
            eprint!("{}", render(&loaded, &diagnostic));
            return 1;
        }
    };
    if let Err(diagnostic) = analyze(&mut program) {
        eprint!("{}", render(&loaded, &diagnostic));
        return 1;
    }
    retain(&mut program);

    let bytes = match capture(&program) {
        Ok(bytes) => bytes,
        Err(error) => {
            eprintln!("kc: cannot serialize the program: {error}");
            return 1;
        }
    };
    let output_path = output
        .map(Path::to_path_buf)
        .unwrap_or_else(|| entry.with_extension(""));
    if same_file(&loaded.files[loaded.entry].path, &output_path) {
        eprintln!("kc: the output path would overwrite the entry file");
        return 1;
    }
    let executable = match std::env::current_exe() {
        Ok(executable) => executable,
        Err(error) => {
            eprintln!("kc: cannot find the kc binary: {error}");
            return 1;
        }
    };
    if let Err(error) = std::fs::copy(&executable, &output_path) {
        eprintln!("kc: cannot write {}: {error}", output_path.display());
        return 1;
    }
    if let Err(error) = append(&output_path, &bytes) {
        eprintln!("kc: cannot write {}: {error}", output_path.display());
        return 1;
    }
    0
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
