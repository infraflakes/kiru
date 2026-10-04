//! Interpreting the process arguments and dispatching one command.
//!
//! `run` first asks whether this executable carries a program trailer; when it
//! does, the words name the program's arguments. Otherwise the words select
//! help, version, or a compilation.

use crate::compiler::DIAGNOSTIC_PREFIX;

use super::parse_arguments::{self, Request};
use super::{compile_request, print_help, run_embedded};

/// Interpret the process arguments and return the process exit code: zero on
/// success, otherwise nonzero.
pub(crate) fn run() -> i32 {
    let words: Vec<String> = std::env::args().skip(1).collect();
    let executable = std::env::current_exe().ok();
    if let Some(bytes) = executable
        .as_deref()
        .and_then(crate::compiler::read_trailer)
    {
        return run_embedded::run(&bytes, &words);
    }
    match parse_arguments::parse(&words) {
        Ok(Request::Help) => {
            print_help::print_help();
            0
        }
        Ok(Request::Version) => {
            print_help::print_version();
            0
        }
        Ok(Request::Compile { entry, output }) => match &executable {
            Some(executable) => compile_request::compile(&entry, output.as_deref(), executable),
            None => {
                eprintln!("{DIAGNOSTIC_PREFIX}: cannot find the {DIAGNOSTIC_PREFIX} binary");
                1
            }
        },
        Err(error) => {
            eprintln!("{DIAGNOSTIC_PREFIX}: {error}");
            1
        }
    }
}
