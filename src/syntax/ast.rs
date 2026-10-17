//! The syntax tree produced by the parser.
//!
//! A `File` is one loaded source file: its top-level items in order. An `item`
//! is a declaration or a `use`. A file belongs to its directory's namespace, so
//! the file carries no namespace of its own. Statements exist only inside
//! function and `match` bodies.

use crate::syntax::Span;

/// One parsed source file: its top-level items in order.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct File {
    pub(crate) items: Vec<Item>,
}

/// A top-level item: a declaration or a `use`.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Item {
    Declaration(Declaration),
    Use(Use),
}

/// One `use a::b;` item. What the path ends at decides what it brings into the
/// namespace that wrote it: a namespace gives the qualifier its last segment
/// spells and joins that namespace's methods to dispatch; a name gives that one
/// name bare.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Use {
    pub(crate) path: Vec<String>,
    pub(crate) span: Span,
}

/// A top-level declaration.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Declaration {
    Function(Function),
    Binding(Binding),
    /// A `rec Name { field: Type, ... };` nominal type declaration.
    Type(TypeDeclaration),
}

/// A `rec Name { field: Type, ... };` or `enum Name { ... };` declaration.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct TypeDeclaration {
    pub(crate) name: String,
    pub(crate) name_span: Span,
    /// Whether the declaration is `pub`, so another namespace may reach it.
    pub(crate) public: bool,
    /// The type parameters in `Name<A, B>`, in order; empty when there are
    /// none.
    pub(crate) parameters: Vec<String>,
    pub(crate) kind: TypeDeclarationKind,
    pub(crate) span: Span,
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
}

/// A `fn name(parameters) -> type? { body };` declaration. An absent return
/// type means the function returns no value. A function whose first parameter
/// accepts a receiver's type may be called as `receiver.name(...)`, whatever the
/// parameter is named.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Function {
    pub(crate) name: String,
    pub(crate) name_span: Span,
    /// Whether the declaration is `pub`, so another namespace may reach it.
    pub(crate) public: bool,
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
    /// Whether the declaration is `pub`, so another namespace may reach it.
    pub(crate) public: bool,
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
    /// An integer literal. A negative number is `Negate` over a literal.
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
    Name {
        path: Vec<String>,
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

/// A binary operator. The set is fixed; the checker gives each one its operand
/// and result types.
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
