mod cli;
mod compile;
mod diagnostics;
mod exec;
mod ir;
mod syntax;

fn main() {
    if let Err(e) = cli::run_cli() {
        match e {
            cli::CliError::Message(message) => eprintln!("Error: {}", message),
            // The failure was already rendered (compile diagnostics, TUI
            // task outcomes); only the exit code remains.
            cli::CliError::Reported => {}
        }
        std::process::exit(1);
    }
}
