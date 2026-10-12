//! The kiru compiler.

mod bytecode;
mod cli;
mod compiler;
mod lock_recovery;
mod model;
mod native_registry;
mod runtime;
mod syntax;

fn main() {
    std::process::exit(cli::run());
}
