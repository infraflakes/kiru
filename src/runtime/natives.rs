//! The native kernel: the small set of operations the OS must provide.
//!
//! Output, files, and text are the areas gathered here; the process and time
//! natives live in the process kernel. A handler answers with a runtime value,
//! or with a reason text that the virtual machine reports under the native's
//! own name.

pub(crate) mod environment;
pub(crate) mod file_system;
pub(crate) mod terminal_output;
pub(crate) mod text_operations;
