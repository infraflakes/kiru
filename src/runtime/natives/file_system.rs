//! The file natives: reading, writing, removing, asking whether a path exists,
//! creating a temporary file, and listing a directory.
//!
//! Every operation except `exists` answers an OS failure as a reason text,
//! which the virtual machine turns into a run failure; `exists` answers with a
//! boolean.

use std::sync::Arc;

use crate::runtime::Value;

/// Read a whole file as text. A path that cannot be read, or whose bytes are
/// not UTF-8, fails the run.
pub(crate) fn read(path: &str) -> Result<Value, String> {
    std::fs::read_to_string(path)
        .map(|text| Value::Text(text.into()))
        .map_err(|error| describe(path, &error))
}

/// Replace a file with `text`, creating it when it is absent.
pub(crate) fn write(path: &str, text: &str) -> Result<Value, String> {
    std::fs::write(path, text)
        .map(|()| Value::Nothing)
        .map_err(|error| describe(path, &error))
}

/// Remove a file.
pub(crate) fn remove(path: &str) -> Result<Value, String> {
    std::fs::remove_file(path)
        .map(|()| Value::Nothing)
        .map_err(|error| describe(path, &error))
}

/// Whether a path exists.
pub(crate) fn exists(path: &str) -> Value {
    Value::Bool(std::path::Path::new(path).exists())
}

/// Create a unique empty file from `template` and return its path. The template
/// must end in six `X`s, which `mkstemp` replaces so the name is unique. The
/// file is created exclusively and is readable only by its owner; the caller
/// owns the file and must remove it.
pub(crate) fn mkstemp(template: &str) -> Result<Value, String> {
    let (file, path) =
        nix::unistd::mkstemp(std::path::Path::new(template)).map_err(|error| error.to_string())?;
    drop(file);
    Ok(Value::Text(path.to_string_lossy().into_owned().into()))
}

/// The names of the entries in a directory, excluding `.` and `..`, in sorted
/// order. A directory that cannot be read fails the run. This is the floor for
/// every directory operation: `glob` and a recursive walk are kiru over it.
pub(crate) fn list(path: &str) -> Result<Value, String> {
    let entries = std::fs::read_dir(path).map_err(|error| describe(path, &error))?;
    let mut names: Vec<String> = Vec::new();
    for entry in entries {
        let entry = entry.map_err(|error| describe(path, &error))?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if name == "." || name == ".." {
            continue;
        }
        names.push(name);
    }
    names.sort();
    let values = names
        .into_iter()
        .map(|name| Value::Text(name.into()))
        .collect();
    Ok(Value::Vec(Arc::new(values)))
}

/// The reason a file operation failed, naming the path it acted on.
fn describe(path: &str, error: &std::io::Error) -> String {
    format!("{path}: {error}")
}
