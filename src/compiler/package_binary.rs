//! Packaging the program in a compiled binary, and recovering it.
//!
//! A compiled binary is a copy of `kc` with the serialized program appended,
//! followed by an eight byte length and a magic marker. At startup the program
//! reads its own executable and looks for that marker. Module values are not
//! part of the payload: the binary evaluates them at startup.

mod binary_trailer;
mod serialize_program;

pub(crate) use binary_trailer::{attach_trailer, read_trailer};
pub(crate) use serialize_program::{deserialize_program, serialize_program};
