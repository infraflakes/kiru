//! The semantic facts of an operator: what its operands must be, what it
//! produces, and how it lowers.
//!
//! How an operator is written and how tightly it binds are syntax facts and
//! live with the syntax tree. The compiler owns the rest, so no parser depends
//! on a compiler model.

use crate::compiler::model::Type;
use crate::syntax::BinaryOp;

/// What the two operands of a binary operator must be.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum BinaryOperands {
    /// Both operands are this one type: arithmetic takes `int` and logic
    /// takes `bool`.
    Fixed(Type),
    /// Both operands have the same type, which must be orderable: `int` or
    /// `txt`.
    Ordered,
    /// Both operands have the same type, whatever it is.
    Same,
}

/// The integer arithmetic an operator performs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Arithmetic {
    Add,
    Subtract,
    Multiply,
    Divide,
    Remainder,
}

/// The comparison an operator performs.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Comparison {
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
    Equal,
    NotEqual,
}

/// How a binary operator lowers.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BinaryInstruction {
    /// Integer arithmetic over both operands.
    Arithmetic(Arithmetic),
    /// A comparison of both operands.
    Comparison(Comparison),
    /// `&&`: a false left operand decides the result.
    ShortCircuitAnd,
    /// `||`: a true left operand decides the result.
    ShortCircuitOr,
}

/// The semantic facts of one binary operator: what it accepts, what it
/// produces, and what it lowers to.
pub(crate) struct BinaryOpFacts {
    /// What the two operands must be.
    pub(crate) operands: BinaryOperands,
    /// The type of the result.
    pub(crate) result: Type,
    /// The instruction the operator lowers to.
    pub(crate) instruction: BinaryInstruction,
}

impl BinaryOp {
    /// The one table of binary operator semantics. The checker and the lowerer
    /// read an operator's operands, result, or instruction from here rather
    /// than spelling its behavior again.
    pub(crate) fn facts(self) -> BinaryOpFacts {
        use BinaryOp::*;
        match self {
            Add => BinaryOpFacts {
                operands: BinaryOperands::Fixed(Type::Int),
                result: Type::Int,
                instruction: BinaryInstruction::Arithmetic(Arithmetic::Add),
            },
            Subtract => BinaryOpFacts {
                operands: BinaryOperands::Fixed(Type::Int),
                result: Type::Int,
                instruction: BinaryInstruction::Arithmetic(Arithmetic::Subtract),
            },
            Multiply => BinaryOpFacts {
                operands: BinaryOperands::Fixed(Type::Int),
                result: Type::Int,
                instruction: BinaryInstruction::Arithmetic(Arithmetic::Multiply),
            },
            Divide => BinaryOpFacts {
                operands: BinaryOperands::Fixed(Type::Int),
                result: Type::Int,
                instruction: BinaryInstruction::Arithmetic(Arithmetic::Divide),
            },
            Remainder => BinaryOpFacts {
                operands: BinaryOperands::Fixed(Type::Int),
                result: Type::Int,
                instruction: BinaryInstruction::Arithmetic(Arithmetic::Remainder),
            },
            Less => BinaryOpFacts {
                operands: BinaryOperands::Ordered,
                result: Type::Bool,
                instruction: BinaryInstruction::Comparison(Comparison::Less),
            },
            LessEqual => BinaryOpFacts {
                operands: BinaryOperands::Ordered,
                result: Type::Bool,
                instruction: BinaryInstruction::Comparison(Comparison::LessEqual),
            },
            Greater => BinaryOpFacts {
                operands: BinaryOperands::Ordered,
                result: Type::Bool,
                instruction: BinaryInstruction::Comparison(Comparison::Greater),
            },
            GreaterEqual => BinaryOpFacts {
                operands: BinaryOperands::Ordered,
                result: Type::Bool,
                instruction: BinaryInstruction::Comparison(Comparison::GreaterEqual),
            },
            Equal => BinaryOpFacts {
                operands: BinaryOperands::Same,
                result: Type::Bool,
                instruction: BinaryInstruction::Comparison(Comparison::Equal),
            },
            NotEqual => BinaryOpFacts {
                operands: BinaryOperands::Same,
                result: Type::Bool,
                instruction: BinaryInstruction::Comparison(Comparison::NotEqual),
            },
            And => BinaryOpFacts {
                operands: BinaryOperands::Fixed(Type::Bool),
                result: Type::Bool,
                instruction: BinaryInstruction::ShortCircuitAnd,
            },
            Or => BinaryOpFacts {
                operands: BinaryOperands::Fixed(Type::Bool),
                result: Type::Bool,
                instruction: BinaryInstruction::ShortCircuitOr,
            },
        }
    }
}
