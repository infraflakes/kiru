//! The native kernel: the small set of operations the OS must provide, and the
//! helpers they share.
//!
//! Output, files, and text are the areas gathered here; the process and time
//! natives live in the process kernel. A handler answers with a runtime value,
//! or with a reason text that the virtual machine reports under the native's
//! own name.

pub(crate) mod environment;
pub(crate) mod file_system;
pub(crate) mod list_operations;
pub(crate) mod terminal_output;
pub(crate) mod text_operations;

/// The text a predicate answers with. The language has no boolean kind, so a
/// predicate returns the exact text `"true"` or `"false"`, the same spelling
/// the record spec entries use.
pub(crate) fn boolean_text(value: bool) -> String {
    if value {
        "true".to_owned()
    } else {
        "false".to_owned()
    }
}

/// Parse a whole number written in text. Indices and lengths are digit texts
/// until the language has a number kind.
pub(crate) fn parse_whole_number(text: &str) -> Result<usize, String> {
    text.parse::<usize>()
        .map_err(|_| format!("`{text}` is not a whole number"))
}
