//! The project manifest: the one file that names a compilation target.
//!
//! A `kiru.toml` names the tree the program is: every `.kiru` file under its
//! `root` joins the program, and the root namespace's `main` is where the
//! compile starts. The manifest also settles the binary's output. The standard
//! library is not a project: the compiler mounts it for every one.

mod discovery;
mod manifest;

pub(crate) use discovery::find;
pub(crate) use manifest::Project;
