//! Loading source files and resolving their imports.
//!
//! The entry file comes from the command line; every `import` loads one more
//! file, depth first and once per canonical path. Importing a file that is
//! already being loaded is an import cycle.

mod load_error;
mod loaded_program;
mod read_entry_and_imports;
mod resolve_import_paths;
#[cfg(test)]
mod tests;

pub(crate) use loaded_program::LoadedProgram;
pub(crate) use read_entry_and_imports::load_files;
