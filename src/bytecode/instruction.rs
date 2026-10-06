//! The lowered instruction set.
//!
//! Each function, module value, and the entry becomes one `Code`; a call names
//! a code id, a local reads a register, and a module value reads a global.

use crate::native_registry::Native;

/// A whole program as bytecode, ready to serialize and run.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub(crate) struct Bytecode {
    pub(crate) codes: Vec<Code>,
    /// The text literals and record keys the instructions reference.
    pub(crate) constants: Vec<String>,
    /// The code id of each module value, in declaration order. The startup pass
    /// runs them once and stores the result in the globals table.
    pub(crate) module_values: Vec<u32>,
    /// The code id of the entry function.
    pub(crate) entry: u32,
}

/// One callable: a function body, a module value initializer, or a defer body.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub(crate) struct Code {
    pub(crate) instructions: Vec<Instruction>,
    /// The number of registers a fresh frame needs. A defer body shares its
    /// enclosing frame, so its own count is unused.
    pub(crate) registers: u16,
    pub(crate) arity: u16,
}

/// One bytecode instruction. A register holds one value.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub(crate) enum Instruction {
    LoadConst {
        dst: u16,
        constant: u32,
    },
    LoadGlobal {
        dst: u16,
        global: u32,
    },
    LoadLocal {
        dst: u16,
        slot: u16,
    },
    StoreLocal {
        slot: u16,
        src: u16,
    },
    BuildRecord {
        dst: u16,
        fields: Vec<(u32, u16)>,
    },
    BuildList {
        dst: u16,
        elements: Vec<u16>,
    },
    /// Read the element at `index` of the list in `list` into `dst`. The index
    /// register holds an internal number.
    ListIndex {
        dst: u16,
        list: u16,
        index: u16,
    },
    /// Jump when the number in `index` has reached the length of the list in
    /// `list`, ending a `for` loop.
    JumpIfIndexAtEnd {
        target: u32,
        index: u16,
        list: u16,
    },
    /// Add one to the internal number in `reg`.
    IncrementNumber {
        reg: u16,
    },
    /// Load an internal number, used only for a loop counter.
    LoadNumber {
        dst: u16,
        value: i64,
    },
    SetField {
        record: u16,
        key: u32,
        value: u16,
    },
    GetField {
        dst: u16,
        record: u16,
        key: u32,
    },
    Concat {
        dst: u16,
        left: u16,
        right: u16,
    },
    CallFunction {
        dst: u16,
        code: u32,
        arguments: Vec<u16>,
    },
    CallNative {
        dst: u16,
        native: Native,
        arguments: Vec<u16>,
    },
    Return {
        value: Option<u16>,
    },
    Panic,
    Jump {
        target: u32,
    },
    JumpIfEqual {
        target: u32,
        left: u16,
        right: u16,
    },
    Async {
        code: u32,
        arguments: Vec<u16>,
    },
    AsyncNative {
        native: Native,
        arguments: Vec<u16>,
    },
    Wait,
}
