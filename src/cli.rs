//! The command line: turning process arguments into a compiler run or into a
//! run of an embedded program.
//!
//! This facade only names the submodules and re-exports the one entry point
//! the binary calls. The trailer detection, argument parsing, help, and
//! compilation each live in their own submodule.

mod compile_request;
mod error;
mod parse_arguments;
mod print_help;
mod run_command;
mod run_embedded;

pub(crate) use error::CliError;
pub(crate) use run_command::run;
