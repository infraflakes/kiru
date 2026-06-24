pub(crate) mod colors;
pub(crate) mod error;
pub(crate) mod exec;
pub(crate) mod output;
pub(crate) mod parse;
pub(crate) mod sync;
pub(crate) mod tui;

#[cfg(test)]
mod tests;

pub(crate) use exec::exec_and_get_stdout;
pub(crate) use output::Runner;
pub(crate) use parse::OutputCallback;
