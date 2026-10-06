//! The environment natives: environment variables and the working directory.

use crate::model::Value;

/// The value of an environment variable, or `""` when it is unset.
pub(crate) fn var(name: &str) -> Value {
    Value::Text(std::env::var(name).unwrap_or_default())
}

/// The process's current working directory.
pub(crate) fn current_dir() -> Result<Value, String> {
    std::env::current_dir()
        .map(|path| Value::Text(path.to_string_lossy().into_owned()))
        .map_err(|error| error.to_string())
}
