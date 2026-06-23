mod cli;
mod colors;
mod config;
mod dsl;
mod ir;
mod runner;
mod shell;
mod sync;
mod tui;

fn main() {
    if let Err(e) = cli::run() {
        eprintln!("{:?}", e);
        std::process::exit(1);
    }
}
