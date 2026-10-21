//! Loading source files.
//!
//! The entry file names the program tree: every `.kiru` file under its
//! directory is loaded, recursively, and a file's namespace is its directory
//! relative to that root. The standard library is loaded from the table
//! compiled into the binary. A file's declarations join its directory's
//! namespace; resolution is order-free.

mod load_error;
mod loaded_program;
mod loader;
#[cfg(test)]
mod tests;

pub(crate) use load_error::LoadError;
pub(crate) use loaded_program::{LoadedProgram, ScopedDeclaration};
pub(in crate::compiler) use loader::collect_kiru_files;
pub(crate) use loader::{PROGRAM_TREE, STD_TREE, load_files, load_single};
