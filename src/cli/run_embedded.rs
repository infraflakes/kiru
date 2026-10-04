//! Running the program embedded in this executable.

use std::sync::Arc;

use crate::compiler::{Program, deserialize_program};
use crate::runtime;

/// Decode the payload and run the embedded program with the words after the
/// program name.
pub(crate) fn run(bytes: &[u8], words: &[String]) -> i32 {
    let program: Program = match deserialize_program(bytes) {
        Ok(program) => program,
        Err(error) => {
            eprintln!("{}: corrupt program: {error}", runtime::program_name());
            return 1;
        }
    };
    if let Err(violation) = crate::compiler::validate_program_structure(&program) {
        eprintln!("{}: corrupt program: {violation}", runtime::program_name());
        return 1;
    }
    runtime::run(Arc::new(program), words)
}
