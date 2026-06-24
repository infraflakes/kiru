pub(crate) mod compile;
pub(crate) mod error;
pub(crate) mod merge;
pub(crate) mod types;
pub(crate) mod validation;

pub use compile::compile;
pub use compile::resolve_includes;
pub use error::CompileError;
pub use types::{Project, Sanctuary};
pub use validation::is_sanctuary_disabled;
pub use validation::validate;

#[cfg(test)]
mod tests;
