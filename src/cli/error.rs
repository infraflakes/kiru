//! A command-line usage error. Its text is printed after the compiler's
//! diagnostic prefix.

/// A command-line usage error.
pub(crate) struct CliError(pub(crate) String);

impl std::fmt::Display for CliError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}
