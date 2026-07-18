mod cli;
mod compiler;
mod dsl;
mod error;
mod runner;
mod shell;

fn main() {
    let _ = miette::set_hook(Box::new(|_| {
        Box::new(miette::MietteHandlerOpts::new().build())
    }));

    if let Err(report) = cli::run_cli() {
        cli::print_diagnostic(&report);
        std::process::exit(1);
    }
}
