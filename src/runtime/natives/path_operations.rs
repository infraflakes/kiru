//! The path natives: splitting a text path into its parts.
//!
//! Paths are text; the operations use the platform's path rules.

use std::path::Path;

use crate::model::Value;

/// The last segment of a path.
pub(crate) fn base(path: &str) -> Value {
    Value::Text(
        Path::new(path)
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default(),
    )
}

/// The path without its last segment.
pub(crate) fn dir(path: &str) -> Value {
    Value::Text(
        Path::new(path)
            .parent()
            .map(|parent| parent.to_string_lossy().into_owned())
            .unwrap_or_default(),
    )
}

/// The extension of a path, without the dot.
pub(crate) fn ext(path: &str) -> Value {
    Value::Text(
        Path::new(path)
            .extension()
            .map(|extension| extension.to_string_lossy().into_owned())
            .unwrap_or_default(),
    )
}
