//! The syntax tree produced by the parser.
//!
//! A `File` is one loaded source file: its top-level items in order. An `item`
//! is a declaration or a `use`. A file belongs to its directory's namespace, so
//! the file carries no namespace of its own. Statements exist only inside
//! function and `match` bodies.

use crate::compiler::model::{DeclarationKind, Type};
use crate::syntax::Span;
use crate::syntax::token::TokenKind;

/// One parsed source file: its top-level items in order.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct File {
    pub(crate) items: Vec<Item>,
}

/// A top-level item: a declaration or a `use`.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Item {
    Declaration(Box<Declaration>),
    Use(Use),
}

/// One `use a::b;` item, or one `use a::{b, c};` group. What each path ends at
/// decides what it brings into the namespace that wrote it: a namespace gives
/// the qualifier its last segment spells; a type gives that type bare; a plain
/// function gives that one name bare.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Use {
    /// The paths the item brings in: one for a plain `use`, one per name for a
    /// one-level `use prefix::{name, name};` group.
    pub(crate) paths: Vec<Vec<String>>,
    pub(crate) span: Span,
}

/// A top-level declaration: its kind, its visibility, the name it introduces,
/// and the form that carries its body.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Declaration {
    /// The kind this declaration is. The parser marks it from the form it
    /// reads, and every later pass reads it instead of matching the form.
    pub(crate) kind: DeclarationKind,
    /// Whether the declaration is `pub`, so another namespace may reach it. A
    /// kind that does not take visibility is never marked `pub` by the parser.
    pub(crate) public: bool,
    pub(crate) name: String,
    pub(crate) name_span: Span,
    pub(crate) form: DeclarationForm,
}

/// The body of a declaration: the one payload its kind carries.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum DeclarationForm {
    Function(Function),
    Binding(Binding),
    Type(TypeDeclaration),
    Impl(ImplBlock),
}

impl Declaration {
    /// The function payload when the form carries one.
    pub(crate) fn function(&self) -> Option<&Function> {
        match &self.form {
            DeclarationForm::Function(function) => Some(function),
            DeclarationForm::Binding(_) | DeclarationForm::Type(_) | DeclarationForm::Impl(_) => {
                None
            }
        }
    }

    /// The binding payload when the form carries one.
    pub(crate) fn binding(&self) -> Option<&Binding> {
        match &self.form {
            DeclarationForm::Binding(binding) => Some(binding),
            DeclarationForm::Function(_) | DeclarationForm::Type(_) | DeclarationForm::Impl(_) => {
                None
            }
        }
    }

    /// The declared type payload when the form carries one.
    pub(crate) fn type_declaration(&self) -> Option<&TypeDeclaration> {
        match &self.form {
            DeclarationForm::Type(declaration) => Some(declaration),
            DeclarationForm::Function(_)
            | DeclarationForm::Binding(_)
            | DeclarationForm::Impl(_) => None,
        }
    }

    /// The `impl` block when the form carries one.
    #[cfg(test)]
    pub(crate) fn impl_block(&self) -> Option<&ImplBlock> {
        match &self.form {
            DeclarationForm::Impl(block) => Some(block),
            DeclarationForm::Function(_)
            | DeclarationForm::Binding(_)
            | DeclarationForm::Type(_) => None,
        }
    }

    /// Take the `impl` block the form carries. Only an `impl` declaration has
    /// one.
    pub(crate) fn into_impl(self) -> Option<ImplBlock> {
        match self.form {
            DeclarationForm::Impl(block) => Some(block),
            DeclarationForm::Function(_)
            | DeclarationForm::Binding(_)
            | DeclarationForm::Type(_) => None,
        }
    }
}

/// An `impl Type { fn ...; };` block. Every function in the block is filed
/// under the block's type: one whose first parameter's type is the block's type
/// is a method, reached as `receiver.name(...)`; every other one is an
/// associated function, reached as `Type.name(...)`.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct ImplBlock {
    /// The type the block's functions are filed under, written as any type
    /// path: full, shortened, or bare.
    pub(crate) ty: TypeExpr,
    /// The functions the block files under its type. Each is already marked
    /// with the impl kind, so a pass never re-derives its kind.
    pub(crate) functions: Vec<Declaration>,
    pub(crate) span: Span,
}

/// A `rec Name { field: Type, ... };` or `enum Name { ... };` declaration.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct TypeDeclaration {
    pub(crate) name: String,
    pub(crate) name_span: Span,
    /// The type parameters in `Name<A, B>`, in order; empty when there are
    /// none.
    pub(crate) parameters: Vec<String>,
    pub(crate) kind: TypeDeclarationKind,
    pub(crate) span: Span,
}

impl TypeDeclaration {
    /// The fields of a `rec` body, or `None` for an `enum`.
    pub(crate) fn fields(&self) -> Option<&[TypeField]> {
        match &self.kind {
            TypeDeclarationKind::Rec(fields) => Some(fields),
            TypeDeclarationKind::Enum(_) => None,
        }
    }

    /// The variants of an `enum` body, or `None` for a `rec`.
    pub(crate) fn variants(&self) -> Option<&[VariantDeclaration]> {
        match &self.kind {
            TypeDeclarationKind::Enum(variants) => Some(variants),
            TypeDeclarationKind::Rec(_) => None,
        }
    }
}

/// The body of a declared type: a fixed set of fields (`rec`) or a set of
/// variants (`enum`). The runtime's scalar types are intrinsics, not
/// declarations, and the compiler registers the runtime's `Vec` type itself,
/// so the parser has no bodyless form.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum TypeDeclarationKind {
    Rec(Vec<TypeField>),
    Enum(Vec<VariantDeclaration>),
}

/// One `Name` or `Name(Type, ...)` variant of an `enum` declaration.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct VariantDeclaration {
    pub(crate) name: String,
    pub(crate) name_span: Span,
    /// Whether the variant is `pub`, so another namespace may construct and
    /// match it.
    pub(crate) public: bool,
    pub(crate) payload: Vec<TypeExpr>,
    pub(crate) span: Span,
}

/// One `name: Type` field of a type declaration.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct TypeField {
    pub(crate) name: String,
    pub(crate) name_span: Span,
    /// Whether the field is `pub`, so another namespace may read and write it.
    pub(crate) public: bool,
    pub(crate) ty: TypeExpr,
    pub(crate) span: Span,
}

/// A written type annotation: the name of a declaration, applied to the type
/// arguments written after it, or a function type.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum TypeExpr {
    /// A named type, resolved through its declaration, optionally applied to
    /// type arguments: `txt`, `Vec<T>`, `Name<A, B>`.
    Named {
        path: Vec<String>,
        arguments: Vec<TypeExpr>,
        span: Span,
    },
    /// A function type: `fn(T1, T2)`, or `fn(T1, T2) -> R` when it returns a
    /// value. An absent arrow means the function returns no value.
    Function {
        parameters: Vec<TypeExpr>,
        returns: Option<Box<TypeExpr>>,
        span: Span,
    },
}

impl TypeExpr {
    pub(crate) fn span(&self) -> Span {
        match self {
            TypeExpr::Function { span, .. } | TypeExpr::Named { span, .. } => *span,
        }
    }

    /// The path a written type spells, when it names a type by path. A
    /// function type is written structurally and names no path.
    pub(crate) fn named_path(&self) -> Option<&[String]> {
        match self {
            TypeExpr::Named { path, .. } => Some(path),
            TypeExpr::Function { .. } => None,
        }
    }
}

/// A `fn name(parameters) -> type? { body };` declaration. An absent return
/// type means the function returns no value. Inside an `impl` block, a function
/// whose first parameter accepts the block's type is a method, called as
/// `receiver.name(...)`; every other function in the block is an associated
/// function, called as `Type.name(...)`.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Function {
    pub(crate) name: String,
    pub(crate) name_span: Span,
    /// The type parameters in `fn name<T, U>(...)`, in order; empty when there
    /// are none.
    pub(crate) type_parameters: Vec<String>,
    pub(crate) parameters: Vec<Parameter>,
    pub(crate) return_type: Option<TypeExpr>,
    pub(crate) body: Vec<Statement>,
    pub(crate) span: Span,
}

/// One `name: type` parameter of a function. A parameter is a local copy: it may
/// be reassigned and have its fields mutated, and the caller's value is
/// unaffected.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Parameter {
    pub(crate) ty: TypeExpr,
    pub(crate) name: String,
    pub(crate) span: Span,
}

/// A `let name: type = expression;` declaration or statement. The initializer is
/// a literal, a name, a field path, or a call returning a value of the declared
/// type.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Binding {
    pub(crate) ty: TypeExpr,
    pub(crate) name: String,
    pub(crate) name_span: Span,
    pub(crate) value: Expression,
    pub(crate) span: Span,
}

/// One `key = expression` entry of a record literal.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Field {
    pub(crate) name: String,
    pub(crate) name_span: Span,
    pub(crate) value: Expression,
    pub(crate) span: Span,
}

/// A parsed statement.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Statement {
    Binding(Binding),
    Assignment {
        name: String,
        name_span: Span,
        value: Expression,
        span: Span,
    },
    /// `name.field = expression;` replaces one field of a map.
    FieldAssignment {
        name: String,
        name_span: Span,
        field: String,
        field_span: Span,
        value: Expression,
        span: Span,
    },
    Expression(Expression),
    /// An early exit. A value return carries the function's declared type; a
    /// valueless return ends a function that declares no return type.
    Return {
        value: Option<Expression>,
        span: Span,
    },
    /// The keyword statement `panic;`, which ends the run.
    Panic {
        span: Span,
    },
    /// The keyword statement `async <call>;`, which spawns the call on a new
    /// thread.
    Async {
        call: Expression,
        span: Span,
    },
    /// The keyword statement `wait;`, which joins the asyncs the calling
    /// thread spawned.
    Wait {
        span: Span,
    },
    /// `for { ... };` runs the body over and over until a `break;`.
    Forever {
        body: Vec<Statement>,
        span: Span,
    },
    /// `break;` ends the nearest enclosing loop early.
    Break {
        span: Span,
    },
}

/// One `pattern => body;` arm of a match. The pattern is an expression: the
/// resolver decides whether it is an equality atom or a variant pattern.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct MatchArm {
    pub(crate) pattern: Expression,
    pub(crate) body: MatchBody,
    pub(crate) span: Span,
}

/// The body of a match arm: an expression in expression position, or a block
/// of statements in statement position.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum MatchBody {
    Expression(Box<Expression>),
    Block(Vec<Statement>),
}

/// A parsed expression.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Expression {
    Text {
        value: String,
        span: Span,
    },
    Map {
        fields: Vec<Field>,
        span: Span,
    },
    /// A `true` or `false` literal.
    Bool {
        value: bool,
        span: Span,
    },
    /// An integer literal. A sign written directly before the digits is the
    /// literal's own sign, so `-1` is one `Int`; `Negate` is for any other
    /// operand.
    Int {
        value: i64,
        span: Span,
    },
    /// A string with `@(…)` interpolation, split into literal and expression
    /// parts. A string with no interpolation is a plain `Text`.
    Interpolated {
        parts: Vec<StringPart>,
        span: Span,
    },
    /// A bare name is a value: `x`, `std::run`.
    Name {
        path: Vec<String>,
        span: Span,
    },
    /// The wildcard `_`, alone in expression position: a match's default arm
    /// reads its kind, and a variant pattern's ignored payload names no
    /// binding. One word, one kind, declared with the tokens.
    Wildcard {
        span: Span,
    },
    Call {
        callee: Vec<String>,
        callee_span: Span,
        arguments: Vec<Expression>,
        span: Span,
    },
    /// `callee(arguments...)` where `callee` is a path: `f()`, `std::run(x)`.
    Field {
        target: Box<Expression>,
        name: String,
        name_span: Span,
        span: Span,
    },
    /// `receiver.method(arguments...)`: a method call.
    MethodCall {
        receiver: Box<Expression>,
        method: String,
        method_span: Span,
        arguments: Vec<Expression>,
        span: Span,
    },
    /// A binary operator applied to two operands.
    Binary {
        op: BinaryOp,
        left: Box<Expression>,
        right: Box<Expression>,
        span: Span,
    },
    /// A prefix operator applied to one operand.
    Unary {
        op: UnaryOp,
        operand: Box<Expression>,
        span: Span,
    },
    /// `match subject { pattern => body; … };` — a term whose value is the
    /// taken arm's expression, or no value when its arms are blocks.
    Match {
        subject: Box<Expression>,
        cases: Vec<MatchArm>,
        default: Option<MatchBody>,
        span: Span,
    },
    /// An anonymous function literal: `fn(a: T) -> R { ... }`. In expression
    /// position it is a closure value that captures the enclosing locals.
    Closure(Box<Closure>),
}

/// An anonymous function literal: its parameters, return type, and body.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Closure {
    pub(crate) parameters: Vec<Parameter>,
    pub(crate) return_type: Option<TypeExpr>,
    pub(crate) body: Vec<Statement>,
    pub(crate) span: Span,
}

impl Expression {
    pub(crate) fn span(&self) -> Span {
        match self {
            Expression::Text { span, .. }
            | Expression::Map { span, .. }
            | Expression::Bool { span, .. }
            | Expression::Int { span, .. }
            | Expression::Interpolated { span, .. }
            | Expression::Name { span, .. }
            | Expression::Wildcard { span }
            | Expression::Call { span, .. }
            | Expression::Field { span, .. }
            | Expression::MethodCall { span, .. }
            | Expression::Binary { span, .. }
            | Expression::Unary { span, .. }
            | Expression::Match { span, .. } => *span,
            Expression::Closure(closure) => closure.span,
        }
    }
}

/// A binary operator. The set is fixed, and every fact about one is declared
/// beside it: the token that writes it, its binding power, the types its
/// operands take, the type of its result, and the instruction it lowers to.
/// The parser, the checker, the linker's receiver typing, and the lowering all
/// read these facts rather than spelling an operator's behavior again.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BinaryOp {
    Add,
    Subtract,
    Multiply,
    Divide,
    Remainder,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
    Equal,
    NotEqual,
    And,
    Or,
}

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

/// Everything one binary operator is: how it is written, how tightly it binds,
/// what it accepts, what it produces, and what it lowers to.
pub(crate) struct BinaryOpFacts {
    /// The token that writes the operator.
    pub(crate) spelling: TokenKind,
    /// The binding power: a higher number binds tighter.
    pub(crate) precedence: u8,
    /// What the two operands must be.
    pub(crate) operands: BinaryOperands,
    /// The type of the result.
    pub(crate) result: Type,
    /// The instruction the operator lowers to.
    pub(crate) instruction: BinaryInstruction,
}

impl BinaryOp {
    /// Every binary operator, in declaration order.
    pub(crate) const ALL: &'static [BinaryOp] = &[
        BinaryOp::Add,
        BinaryOp::Subtract,
        BinaryOp::Multiply,
        BinaryOp::Divide,
        BinaryOp::Remainder,
        BinaryOp::Less,
        BinaryOp::LessEqual,
        BinaryOp::Greater,
        BinaryOp::GreaterEqual,
        BinaryOp::Equal,
        BinaryOp::NotEqual,
        BinaryOp::And,
        BinaryOp::Or,
    ];

    /// The one table of binary operator facts. Every stage reads an operator's
    /// spelling, precedence, operands, result, or instruction from here.
    pub(crate) fn facts(self) -> BinaryOpFacts {
        use BinaryOp::*;
        match self {
            Add => BinaryOpFacts {
                spelling: TokenKind::Plus,
                precedence: 5,
                operands: BinaryOperands::Fixed(Type::Int),
                result: Type::Int,
                instruction: BinaryInstruction::Arithmetic(Arithmetic::Add),
            },
            Subtract => BinaryOpFacts {
                spelling: TokenKind::Minus,
                precedence: 5,
                operands: BinaryOperands::Fixed(Type::Int),
                result: Type::Int,
                instruction: BinaryInstruction::Arithmetic(Arithmetic::Subtract),
            },
            Multiply => BinaryOpFacts {
                spelling: TokenKind::Star,
                precedence: 6,
                operands: BinaryOperands::Fixed(Type::Int),
                result: Type::Int,
                instruction: BinaryInstruction::Arithmetic(Arithmetic::Multiply),
            },
            Divide => BinaryOpFacts {
                spelling: TokenKind::Slash,
                precedence: 6,
                operands: BinaryOperands::Fixed(Type::Int),
                result: Type::Int,
                instruction: BinaryInstruction::Arithmetic(Arithmetic::Divide),
            },
            Remainder => BinaryOpFacts {
                spelling: TokenKind::Percent,
                precedence: 6,
                operands: BinaryOperands::Fixed(Type::Int),
                result: Type::Int,
                instruction: BinaryInstruction::Arithmetic(Arithmetic::Remainder),
            },
            Less => BinaryOpFacts {
                spelling: TokenKind::Less,
                precedence: 4,
                operands: BinaryOperands::Ordered,
                result: Type::Bool,
                instruction: BinaryInstruction::Comparison(Comparison::Less),
            },
            LessEqual => BinaryOpFacts {
                spelling: TokenKind::LessEq,
                precedence: 4,
                operands: BinaryOperands::Ordered,
                result: Type::Bool,
                instruction: BinaryInstruction::Comparison(Comparison::LessEqual),
            },
            Greater => BinaryOpFacts {
                spelling: TokenKind::Greater,
                precedence: 4,
                operands: BinaryOperands::Ordered,
                result: Type::Bool,
                instruction: BinaryInstruction::Comparison(Comparison::Greater),
            },
            GreaterEqual => BinaryOpFacts {
                spelling: TokenKind::GreaterEq,
                precedence: 4,
                operands: BinaryOperands::Ordered,
                result: Type::Bool,
                instruction: BinaryInstruction::Comparison(Comparison::GreaterEqual),
            },
            Equal => BinaryOpFacts {
                spelling: TokenKind::EqEq,
                precedence: 3,
                operands: BinaryOperands::Same,
                result: Type::Bool,
                instruction: BinaryInstruction::Comparison(Comparison::Equal),
            },
            NotEqual => BinaryOpFacts {
                spelling: TokenKind::NotEq,
                precedence: 3,
                operands: BinaryOperands::Same,
                result: Type::Bool,
                instruction: BinaryInstruction::Comparison(Comparison::NotEqual),
            },
            And => BinaryOpFacts {
                spelling: TokenKind::AndAnd,
                precedence: 2,
                operands: BinaryOperands::Fixed(Type::Bool),
                result: Type::Bool,
                instruction: BinaryInstruction::ShortCircuitAnd,
            },
            Or => BinaryOpFacts {
                spelling: TokenKind::OrOr,
                precedence: 1,
                operands: BinaryOperands::Fixed(Type::Bool),
                result: Type::Bool,
                instruction: BinaryInstruction::ShortCircuitOr,
            },
        }
    }
}

/// A prefix operator.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum UnaryOp {
    Negate,
    Not,
}

/// One part of an interpolated string.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum StringPart {
    /// Literal text.
    Literal(String),
    /// An `@(…)` expression whose text is inserted.
    Expression(Expression),
}
