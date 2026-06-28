mod cli;
mod compiler;
mod dsl;
mod runner;
mod shell;

fn main() {
    if let Err(e) = cli::run_cli() {
        eprintln!("{:?}", e);
        std::process::exit(1);
    }
}
