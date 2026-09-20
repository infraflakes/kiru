//! Loading source files and resolving their imports.
//!
//! The entry file comes from the command line; every `import` loads one more
//! file, depth first and once per canonical path. Importing a file that is
//! already being loaded is an import cycle.

mod error;
mod files;
mod imports;
mod loading;
#[cfg(test)]
mod tests;

pub(crate) use files::{LoadedProgram, Origin};
pub(crate) use loading::load;
