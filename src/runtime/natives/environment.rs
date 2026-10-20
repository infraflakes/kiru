//! The environment natives: environment variables and the working directory.

use crate::runtime::Value;
use crate::runtime::natives::{option_none, option_some};

/// The value of an environment variable, or `std::Option.none` when it is
/// unset. An unset variable is not empty text.
pub(crate) fn var(name: &str) -> Value {
    match std::env::var(name) {
        Ok(value) => option_some(Value::Text(value.into())),
        Err(_) => option_none(),
    }
}

/// The process's current working directory.
pub(crate) fn current_dir() -> Result<Value, String> {
    std::env::current_dir()
        .map(|path| Value::Text(path.to_string_lossy().into_owned().into()))
        .map_err(|error| error.to_string())
}
