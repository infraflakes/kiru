//! Interpreting the process arguments and dispatching one command.
//!
//! `run` first asks whether this executable carries a program trailer; when it
//! does, the words name the program's arguments. Otherwise the words select
//! help, version, a compilation, or the language server.

use crate::compiler::DIAGNOSTIC_PREFIX;

use super::parse_arguments::{self, Request};
use super::{compile_request, print_help, run_embedded};

/// Interpret the process arguments and return the process exit code: zero on
/// success, otherwise nonzero.
pub fn run() -> i32 {
    let words: Vec<String> = std::env::args().skip(1).collect();
    let executable = std::env::current_exe().ok();
    // The bytes of the running executable are the command line's to read; the
    // compiler names the format they carry.
    if let Some(payload) = executable
        .as_deref()
        .and_then(|path| std::fs::read(path).ok())
        .as_deref()
        .and_then(crate::compiler::payload_in)
    {
        return run_embedded::run(payload, &words);
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
        Ok(Request::Serve) => {
            crate::lsp::serve();
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
