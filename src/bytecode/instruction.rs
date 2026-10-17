//! The lowered instruction set.
//!
//! Each function, module value, and the entry becomes one `Code`; a call names
//! a code id, a local reads a register, and a module value reads a global.

use crate::native_registry::Native;

/// A whole program as bytecode, ready to serialize and run.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub(crate) struct Bytecode {
    pub(crate) codes: Vec<Code>,
    /// The text literals and map keys the instructions reference.
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
    BuildMap {
        dst: u16,
        fields: Vec<(u32, u16)>,
    },
    /// Construct an empty Vec in `dst`: the runtime's list constructor, which
    /// `Vec.new()` lowers to.
    VecNew {
        dst: u16,
    },
    /// Read the element at `index` of the Vec in `vec` into `dst`: the runtime
    /// of `xs.at(index)`. The index register holds an `int`; an index outside
    /// the Vec is a run failure.
    VecAt {
        dst: u16,
        vec: u16,
        index: u16,
    },
    /// Replace the element at `index` of the Vec in `vec`: the runtime of
    /// `xs.replace(index, item)`. The index must be inside the Vec; it never
    /// appends.
    VecReplace {
        vec: u16,
        index: u16,
        value: u16,
    },
    /// Append the value in `item` to the Vec in `vec`: the runtime of
    /// `xs.push(item)`.
    VecPush {
        vec: u16,
        item: u16,
    },
    /// The number of elements of the Vec in `vec` into `dst`: the runtime of
    /// `xs.len()`.
    VecLen {
        dst: u16,
        vec: u16,
    },
    /// Construct an enum variant: `dst = Variant(tag, payload...)`.
    MakeVariant {
        dst: u16,
        tag: u16,
        payload: Vec<u16>,
    },
    /// Load the absence into `dst`: the `none` of the declared absence type and
    /// the no-value result of a call that returns nothing. There is one
    /// absence.
    LoadNothing {
        dst: u16,
    },
    /// Jump when the value is not a variant with `tag`; used by a match arm.
    JumpIfVariantNotEqual {
        target: u16,
        tag: u16,
        jump: u32,
    },
    /// Jump when the value is the absence; the `none` arm of a match.
    JumpIfNothing {
        value: u16,
        jump: u32,
    },
    /// Jump when the value is present; the `some` arm of a match.
    JumpIfNotNothing {
        value: u16,
        jump: u32,
    },
    /// Extract payload position `index` of a variant into `dst`.
    GetVariantField {
        dst: u16,
        variant: u16,
        index: u16,
    },
    /// Make a function value: `dst = <function code>`. A closure also carries
    /// the values of the locals it captured, in `captures`.
    MakeFunction {
        dst: u16,
        code: u32,
        captures: Vec<u16>,
    },
    /// Call a function value: `dst = callee(arguments...)`.
    CallValue {
        dst: u16,
        callee: u16,
        arguments: Vec<u16>,
    },
    /// Load an integer literal.
    LoadNumber {
        dst: u16,
        value: i64,
    },
    /// Load a boolean literal.
    LoadBool {
        dst: u16,
        value: bool,
    },
    /// Integer arithmetic: both operands are integers and the result is one.
    NumberOp {
        op: NumberOp,
        dst: u16,
        left: u16,
        right: u16,
    },
    /// Negate the integer in `src` into `dst`.
    Negate {
        dst: u16,
        src: u16,
    },
    /// The boolean not of `src` into `dst`.
    Not {
        dst: u16,
        src: u16,
    },
    /// Compare two values into a boolean.
    Compare {
        op: CompareOp,
        dst: u16,
        left: u16,
        right: u16,
    },
    /// Jump when the boolean in `condition` is false, which short-circuits
    /// `&&` and `||`.
    JumpIfFalse {
        target: u32,
        condition: u16,
    },
    /// Format a value as text: text as is, an integer in decimal, a boolean as
    /// `true` or `false`.
    ToText {
        dst: u16,
        src: u16,
    },
    SetField {
        map: u16,
        key: u32,
        value: u16,
    },
    GetField {
        dst: u16,
        map: u16,
        key: u32,
    },
    Concat {
        dst: u16,
        left: u16,
        right: u16,
    },
    CallNative {
        dst: u16,
        native: Native,
        arguments: Vec<u16>,
    },
    /// Return the value in `value`. A `return;` in a function that returns
    /// nothing loads the absence first, so a return always names one register.
    Return {
        value: u16,
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
    AsyncNative {
        native: Native,
        arguments: Vec<u16>,
    },
    /// Spawn a thread that calls a function value: `callee(arguments...)`.
    AsyncValue {
        callee: u16,
        arguments: Vec<u16>,
    },
    Wait,
}

/// An integer operation.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub(crate) enum NumberOp {
    Add,
    Subtract,
    Multiply,
    Divide,
    Remainder,
}

/// A comparison. Ordering compares integers; equality compares any two values
/// of the same type.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub(crate) enum CompareOp {
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
    Equal,
    NotEqual,
}
