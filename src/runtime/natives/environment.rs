//! The environment natives: environment variables and the working directory.

use crate::runtime::Value;

/// The value of an environment variable, or the absence when it is unset.
pub(crate) fn var(name: &str) -> Value {
    match std::env::var(name) {
        Ok(value) => Value::Text(value.into()),
        Err(_) => Value::Nothing,
    }
}

/// The process's current working directory.
pub(crate) fn current_dir() -> Result<Value, String> {
    std::env::current_dir()
        .map(|path| Value::Text(path.to_string_lossy().into_owned().into()))
        .map_err(|error| error.to_string())
}
