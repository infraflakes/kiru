mod cli;
mod diagnostics;
mod exec;
mod ir;
mod lower;
mod syntax;

fn main() {
    if let Err(e) = cli::run_cli() {
        if !e.is_empty() {
            eprintln!("Error: {}", e);
        }
        std::process::exit(1);
    }
}
