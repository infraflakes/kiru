//! Serializing the bytecode into a payload and recovering it.

use crate::bytecode::Bytecode;

/// Serialize a lowered program to the payload a compiled binary carries.
pub(crate) fn serialize_bytecode(bytecode: &Bytecode) -> Result<Vec<u8>, String> {
    postcard::to_allocvec(bytecode).map_err(|error| error.to_string())
}

/// Recover the bytecode from a payload read out of an executable.
pub(crate) fn deserialize_bytecode(bytes: &[u8]) -> Result<Bytecode, String> {
    postcard::from_bytes(bytes).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use crate::compiler::{checked_program, lower_bytecode};

    #[test]
    fn bytecode_round_trips_through_bytes() {
        let program = checked_program(
            "rec p = { dir = \"~/x\" };\n\
             txt title = \"kiru\";\n\
             fn main() { std::io::print(title); };",
        );
        let bytecode = lower_bytecode(&program);

        let bytes = super::serialize_bytecode(&bytecode).expect("serializes");
        let restored = super::deserialize_bytecode(&bytes).expect("deserializes");
        assert_eq!(restored.entry, bytecode.entry);
        assert_eq!(restored.codes.len(), bytecode.codes.len());
        assert_eq!(restored.module_values.len(), bytecode.module_values.len());
    }
}
