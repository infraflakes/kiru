//! Embedding the program in a compiled binary, and recovering it.
//!
//! A compiled binary is a copy of `kc` with the serialized program appended,
//! followed by an eight byte length and a magic marker. At startup the
//! program reads its own executable and looks for that marker. Module values
//! are not part of the payload: the binary evaluates them at startup.

mod payload;
mod trailer;

pub(crate) use payload::{capture, restore};
pub(crate) use trailer::{append, read};
