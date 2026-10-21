//! Lowering: the checked model becomes the instruction stream.
//!
//! One pass per direction: `expressions` turns a body's nodes into
//! instructions, and `bytecode` lowers the declarations the entry reaches.
//! Nothing here names a protocol or a file; the model is the only input.

mod bytecode;
mod expressions;

#[cfg(test)]
pub(crate) use bytecode::declaration_codes;
pub(crate) use bytecode::lower_bytecode;
