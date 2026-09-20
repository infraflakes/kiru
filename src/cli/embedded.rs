//! Running the program embedded in this executable.

use std::sync::Arc;

use crate::compiler::{Program, restore};
use crate::runtime;

/// Decode the payload and run the embedded program with the words after the
/// program name.
pub(crate) fn run(bytes: &[u8], words: &[String]) -> i32 {
    let program: Program = match restore(bytes) {
        Ok(program) => program,
        Err(error) => {
            eprintln!("kc: corrupt program: {error}");
            return 1;
        }
    };
    let default_shell = std::env::var("SHELL").ok();
    runtime::run(Arc::new(program), words, default_shell)
}
