pub(crate) mod compile;
pub(crate) mod error;
pub(crate) mod merge;
pub(crate) mod resolve;
pub(crate) mod types;
pub(crate) mod validation;

/// Parse and compile a kiru config file at the given entry path, returning a fully resolved
/// [`Sanctuary`] with all imports merged and shell variables evaluated.
pub use compile::compile;
/// Re-read each project's include file and merge its stmts into the project definition.
/// Requires that the sanctuary path has been synced first.
pub use compile::resolve_includes;
pub use error::CompileError;
pub use types::{Project, Sanctuary, SyncMode};
/// Returns true when the `SANCTUARY` environment variable is set to `0`,
/// which disables sanctuary path and project field requirements.
pub use validation::is_sanctuary_disabled;
pub use validation::validate;

#[cfg(test)]
mod tests;
