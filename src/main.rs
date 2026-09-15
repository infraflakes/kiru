//! The kiru compiler.

mod cli;
mod compiler;
mod runtime;
mod syntax;

fn main() {
    std::process::exit(cli::run());
}
