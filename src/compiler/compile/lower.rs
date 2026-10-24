//! Lowering: the checked model becomes the instruction stream.
//!
//! One pass per direction: `expressions` turns a body's nodes into
//! instructions, and `program` lowers the declarations the entry reaches.
//! Nothing here names a protocol or a file; the model and the check's facts
//! are the only inputs.

mod expressions;
mod program;

#[cfg(test)]
pub(crate) use program::declaration_codes;
pub(crate) use program::lower_bytecode;
