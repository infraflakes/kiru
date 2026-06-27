pub(crate) mod compile;
pub(crate) mod error;
pub(crate) mod merge;
pub(crate) mod resolve;
pub(crate) mod types;
pub(crate) mod validation;

/// Run the full pipeline: parse, merge, resolve includes, validate, and resolve.
pub use compile::compile_and_resolve;
pub use error::CompileError;
pub use types::{
    Project, ResolvedCasePattern, ResolvedEnvPair, ResolvedFnStmt, Sanctuary, SyncMode,
};
/// Returns true when the `SANCTUARY` environment variable is set to `0`.
pub use validation::is_sanctuary_disabled;

#[cfg(test)]
mod tests;
