//! The command line: turning process arguments into a compiler run or into a
//! run of an embedded program.
//!
//! `run` is the entry point the binary calls. It first asks whether this
//! executable carries a program trailer; when it does, the words name the
//! program's arguments. Otherwise the words select help, version, or a
//! compilation.

mod args;
mod compile;
mod embedded;
mod help;

use args::Request;

/// A command-line usage error. Its text is printed after the `kc: ` prefix.
pub(crate) struct CliError(pub(crate) String);

impl std::fmt::Display for CliError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(&self.0)
    }
}

/// Interpret the process arguments and return the process exit code: zero on
/// success, otherwise nonzero.
pub(crate) fn run() -> i32 {
    let words: Vec<String> = std::env::args().skip(1).collect();
    if let Ok(executable) = std::env::current_exe()
        && let Some(bytes) = crate::compiler::read(&executable)
    {
        return embedded::run(&bytes, &words);
    }
    match args::parse(&words) {
        Ok(Request::Help) => {
            help::print_help();
            0
        }
        Ok(Request::Version) => {
            help::print_version();
            0
        }
        Ok(Request::Compile { entry, output }) => compile::compile(&entry, output.as_deref()),
        Err(error) => {
            eprintln!("kc: {error}");
            1
        }
    }
}
