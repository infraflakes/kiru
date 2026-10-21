//! Packaging the program in a compiled binary, and recovering it.
//!
//! A compiled binary is a copy of `kc` with the serialized bytecode appended,
//! followed by an eight byte length and a magic marker. At startup the program
//! reads its own executable and looks for that marker. Module values are not
//! part of the payload: the binary evaluates them at startup.

mod binary_trailer;
mod output_file;
mod payload;

pub(crate) use binary_trailer::{attach_trailer, payload_in};
pub(crate) use output_file::make_writable_executable;
pub(crate) use payload::{deserialize_bytecode, serialize_bytecode};
