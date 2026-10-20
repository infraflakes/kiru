//! Running the program embedded in this executable.

use std::sync::Arc;

use crate::compiler::bytecode::{Bytecode, validate_bytecode_structure};
use crate::compiler::deserialize_bytecode;
use crate::runtime;

/// The name this program reports under: the invoked binary's file name, or
/// `kiru` when the name cannot be read.
fn program_name() -> String {
    std::env::args_os()
        .next()
        .and_then(|argument| {
            std::path::Path::new(&argument)
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
        })
        .filter(|name| !name.is_empty())
        .unwrap_or_else(|| "kiru".to_owned())
}

/// Decode the payload and run the embedded program with the words after the
/// program name.
pub(crate) fn run(bytes: &[u8], words: &[String]) -> i32 {
    let bytecode: Bytecode = match deserialize_bytecode(bytes) {
        Ok(bytecode) => bytecode,
        Err(error) => {
            eprintln!("{}: corrupt program: {error}", program_name());
            return 1;
        }
    };
    if let Err(violation) = validate_bytecode_structure(&bytecode) {
        eprintln!("{}: corrupt program: {violation}", program_name());
        return 1;
    }
    runtime::run(Arc::new(bytecode), words)
}
