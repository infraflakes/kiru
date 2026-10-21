//! Loading source files.
//!
//! The target names the program tree: every `.kiru` file under its directory is
//! loaded, recursively, and a file's namespace is its directory relative to
//! that root. The standard library is loaded from the table compiled into the
//! binary. A single file is loaded in its own context: beside the library it
//! belongs to, or beside the embedded one. Every byte comes through the program
//! source; a file's declarations join its directory's namespace, and resolution
//! is order-free.

mod embedded;
mod load_error;
mod loaded_program;
mod record;
mod sources;
#[cfg(test)]
mod tests;
mod tree;

pub(crate) use load_error::LoadError;
pub(crate) use loaded_program::{LoadedProgram, ScopedDeclaration};
pub use sources::ProgramSource;
pub(crate) use tree::{PROGRAM_TREE, STD_TREE, load_files, load_single};
