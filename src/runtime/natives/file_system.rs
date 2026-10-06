//! The file natives: reading, writing, removing, asking whether a path exists,
//! creating a temporary file, and matching a glob.
//!
//! Every operation except `exists` answers an OS failure as a reason text,
//! which the virtual machine turns into a run failure; `exists` answers with
//! text.

use crate::model::Value;

use super::boolean_text;

/// Read a whole file as text. A path that cannot be read, or whose bytes are
/// not UTF-8, fails the run.
pub(crate) fn read(path: &str) -> Result<Value, String> {
    std::fs::read_to_string(path)
        .map(Value::Text)
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
    Value::Text(boolean_text(std::path::Path::new(path).exists()))
}

/// Create a unique empty file and return its path. The file is created with
/// `mkstemp`, so it is exclusive and readable only by its owner; the caller
/// owns the file and must remove it.
pub(crate) fn temp_file() -> Result<Value, String> {
    let template = std::env::temp_dir().join("kiru-XXXXXX");
    let (file, path) = nix::unistd::mkstemp(&template).map_err(|error| error.to_string())?;
    drop(file);
    Ok(Value::Text(path.to_string_lossy().into_owned()))
}

/// Expand a glob pattern into a list of matching paths, in sorted order. The
/// pattern uses `*`, `?`, `[...]`, and `**` as the `glob` crate defines them.
pub(crate) fn glob(pattern: &str) -> Result<Value, String> {
    let paths = glob::glob(pattern).map_err(|error| error.to_string())?;
    let mut matches = Vec::new();
    for entry in paths {
        let path = entry.map_err(|error| error.to_string())?;
        matches.push(path.to_string_lossy().into_owned());
    }
    Ok(Value::List(matches))
}

/// The reason a file operation failed, naming the path it acted on.
fn describe(path: &str, error: &std::io::Error) -> String {
    format!("{path}: {error}")
}
