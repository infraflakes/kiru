//! The lowered instruction set.
//!
//! Each function, module value, and the entry becomes one `Code`; a call names
//! a code id, a local reads a register, and a module value reads a global.

use crate::compiler::native_registry::{Native, NativeSignature};

/// A whole program as bytecode, ready to serialize and run.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub(crate) struct Bytecode {
    pub(crate) codes: Vec<Code>,
    /// The text literals and map keys the instructions reference.
    pub(crate) constants: Vec<String>,
    /// The startup plan for module values, in evaluation order. Each entry runs
    /// its code once and stores the result in its global.
    pub(crate) module_values: Vec<ModuleValue>,
    /// The code id of the entry function.
    pub(crate) entry: u32,
    /// Every native's declared parameter shapes, in declaration order. The
    /// runtime decodes a native call from its row rather than a per-handler
    /// stack discipline.
    pub(crate) natives: Vec<NativeSignature>,
}

/// One module value's startup entry: the code that evaluates its initializer
/// and the global that receives the value. The code is chosen in evaluation
/// order and the global in declaration order, so the two are one entry rather
/// than one numbering forced to serve both.
#[derive(Debug, serde::Serialize, serde::Deserialize)]
pub(crate) struct ModuleValue {
    pub(crate) code: u32,
    pub(crate) global: u32,
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
    /// Construct an enum variant: `dst = Variant(tag, payload...)`.
    MakeVariant {
        dst: u16,
        tag: u16,
        payload: Vec<u16>,
    },
    /// Load the no-value marker into `dst`: what a bare `return;` returns from
    /// a function that declares no return type, and what a native with no
    /// result produces. It is not a value of any type.
    LoadNothing {
        dst: u16,
    },
    /// Jump when the value is not a variant with `tag`; used by a match arm.
    JumpIfVariantNotEqual {
        target: u16,
        tag: u16,
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
    /// nothing loads the no-value marker first, so a return always names one
    /// register.
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

impl From<crate::syntax::Arithmetic> for NumberOp {
    /// The instruction an operator's arithmetic fact names. The mapping lives
    /// beside the instruction set, so the operator table names no instruction
    /// variant itself.
    fn from(arithmetic: crate::syntax::Arithmetic) -> Self {
        use crate::syntax::Arithmetic;
        match arithmetic {
            Arithmetic::Add => NumberOp::Add,
            Arithmetic::Subtract => NumberOp::Subtract,
            Arithmetic::Multiply => NumberOp::Multiply,
            Arithmetic::Divide => NumberOp::Divide,
            Arithmetic::Remainder => NumberOp::Remainder,
        }
    }
}

impl From<crate::syntax::Comparison> for CompareOp {
    /// The instruction an operator's comparison fact names.
    fn from(comparison: crate::syntax::Comparison) -> Self {
        use crate::syntax::Comparison;
        match comparison {
            Comparison::Less => CompareOp::Less,
            Comparison::LessEqual => CompareOp::LessEqual,
            Comparison::Greater => CompareOp::Greater,
            Comparison::GreaterEqual => CompareOp::GreaterEqual,
            Comparison::Equal => CompareOp::Equal,
            Comparison::NotEqual => CompareOp::NotEqual,
        }
    }
}
