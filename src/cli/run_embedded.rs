//! Running the program embedded in this executable.

use std::sync::Arc;

use crate::bytecode::{Bytecode, validate_bytecode_structure};
use crate::compiler::deserialize_bytecode;
use crate::runtime;

/// Decode the payload and run the embedded program with the words after the
/// program name.
pub(crate) fn run(bytes: &[u8], words: &[String]) -> i32 {
    let bytecode: Bytecode = match deserialize_bytecode(bytes) {
        Ok(bytecode) => bytecode,
        Err(error) => {
            eprintln!("{}: corrupt program: {error}", runtime::program_name());
            return 1;
        }
    };
    if let Err(violation) = validate_bytecode_structure(&bytecode) {
        eprintln!("{}: corrupt program: {violation}", runtime::program_name());
        return 1;
    }
    runtime::run(Arc::new(bytecode), words)
}
