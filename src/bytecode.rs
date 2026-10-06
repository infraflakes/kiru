//! The bytecode a compiled program runs.
//!
//! The validated graph is lowered to a compact instruction stream at compile
//! time. Each function, module value, and the entry becomes one `Code`; a call
//! names a code id, a local reads a register, and a module value reads a
//! global. The runtime is a register machine over these instructions.

mod instruction;
mod validation;

pub(crate) use instruction::{Bytecode, Code, Instruction};
pub(crate) use validation::validate_bytecode_structure;
