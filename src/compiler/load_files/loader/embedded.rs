//! The standard library's files, compiled into every program.
//!
//! The table is generated from `std/**` at build time: one entry per `.kiru`
//! file, keyed by the path its directory gives it. A new library file is
//! embedded without a second declaration, because the directory is the
//! declaration.

include!(concat!(env!("OUT_DIR"), "/embedded_library.rs"));
