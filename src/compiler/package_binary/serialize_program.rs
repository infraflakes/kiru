//! Serializing the program into a payload and recovering it.

use crate::compiler::Program;

/// Serialize a linked and checked program to the payload a compiled binary
/// carries.
pub(crate) fn serialize_program(program: &Program) -> Result<Vec<u8>, String> {
    postcard::to_allocvec(program).map_err(|error| error.to_string())
}

/// Recover the program from a payload read out of an executable.
pub(crate) fn deserialize_program(bytes: &[u8]) -> Result<Program, String> {
    postcard::from_bytes(bytes).map_err(|error| error.to_string())
}

#[cfg(test)]
mod tests {
    use crate::compiler::{load_files, resolve_names, validate_program};

    #[test]
    fn program_round_trips_through_bytes() {
        let directory = tempfile::tempdir().expect("temp dir");
        let path = directory.path().join("main.kiru");
        std::fs::write(
            &path,
            "rec p = { dir = \"~/x\" };\n\
             txt title = \"kiru\";\n\
             fn main() { std::print(title); };",
        )
        .expect("write file");
        let mut loaded = load_files::load_files(&path).expect("loads");
        let mut program = resolve_names::resolve_names(&mut loaded).expect("links");
        validate_program::validate_program(&mut program).expect("checks");

        let bytes = super::serialize_program(&program).expect("serializes");
        let restored = super::deserialize_program(&bytes).expect("deserializes");
        assert_eq!(restored.entry, program.entry);
        assert_eq!(restored.declarations.len(), program.declarations.len());
        assert!(
            restored.values.read().expect("unpoisoned").is_empty(),
            "the payload must not carry module values"
        );
    }
}
