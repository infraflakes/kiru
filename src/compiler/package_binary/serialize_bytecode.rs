//! Serializing the bytecode into a payload and recovering it.

use crate::compiler::bytecode::Bytecode;

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
    use crate::compiler::model::name_hash;
    use crate::compiler::{checked_program, lower_bytecode};

    /// The same source as the round-trip test, so one program is pinned twice:
    /// its bytes and their count.
    const SOURCE: &str = "use std;\nuse std::text;\nuse std::dynamic;\n\
        rec P { dir: txt };\n\
        let p: P = { dir = \"~/x\" };\n\
        let title: txt = \"kiru\";\n\
        fn main() { std::print(title); };";

    #[test]
    fn bytecode_round_trips_through_bytes() {
        let program = checked_program(SOURCE);
        let bytecode = lower_bytecode(&program);

        let bytes = super::serialize_bytecode(&bytecode).expect("serializes");
        let restored = super::deserialize_bytecode(&bytes).expect("deserializes");
        assert_eq!(restored.entry, bytecode.entry);
        assert_eq!(restored.codes.len(), bytecode.codes.len());
        assert_eq!(restored.module_values.len(), bytecode.module_values.len());
    }

    /// The compiled image of one fixed program is byte-identical: the
    /// serialization is the contract the runtime reads, so it pins here.
    #[test]
    fn the_compiled_image_is_byte_identical() {
        let bytes = super::serialize_bytecode(&lower_bytecode(&checked_program(SOURCE)))
            .expect("serializes");
        assert_eq!(bytes.len(), 17675);
        assert_eq!(name_hash(&bytes), 12749693456951739665);
    }
}
