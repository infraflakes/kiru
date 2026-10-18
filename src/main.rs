//! The kiru compiler.

mod cli;
mod compiler;
mod lock_recovery;
mod native_registry;
mod runtime;
mod syntax;

fn main() {
    std::process::exit(cli::run());
}
