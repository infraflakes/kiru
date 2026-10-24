//! The linked expressions and statements. Every name is already an edge to a
//! declaration node.

use crate::compiler::model::{Shape, Type, TypeId};
use crate::syntax::{BinaryOp, Span, UnaryOp};

use super::ids::{DeclarationId, NamespaceId};

/// A function body: a named `fn` declaration's body, or the body of the
/// anonymous function a `[...] { .. }` literal declares. The same node either
/// way; the holding declaration's binding names a namespace name or a
/// containing body, and only an anonymous function captures an environment.
#[derive(Debug)]
pub(crate) struct Function {
    /// The return type the literal declares in its own annotation. A named
    /// function's return type is the one its declaration carries, so its node
    /// declares none here; only a literal's own annotation is stored.
    pub(crate) declared_return: Option<Type>,
    /// The parameter nodes, in order.
    pub(crate) parameters: Vec<DeclarationId>,
    pub(crate) body: Vec<Statement>,
    /// The type parameters in `fn name<T, ...>`, in order; empty when there are
    /// none.
    pub(crate) type_parameters: Vec<String>,
    /// The enclosing locals the body reads and copies in. Empty for a function
    /// that reads no enclosing local.
    pub(crate) captures: Vec<DeclarationId>,
    pub(crate) span: Span,
}

/// A linked expression. Every name is already an edge to a node.
#[derive(Debug)]
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
    /// An integer literal.
    Int {
        value: i64,
        span: Span,
    },
    /// A constructed enum variant: `Name.Variant(payload, ...)`.
    Variant {
        ty: TypeId,
        index: usize,
        payload: Vec<Expression>,
        span: Span,
    },
    /// A string with `@(…)` interpolation, split into literal and expression
    /// parts.
    Interpolated {
        parts: Vec<StringPart>,
        span: Span,
    },
    /// A reference to a declaration or a binding, which is also a function
    /// value when the declaration's type is a function type. Resolution turns
    /// the written path into this edge; a path begins with a name, since a
    /// leading `::` is not a form.
    Reference {
        declaration: DeclarationId,
        span: Span,
    },
    /// An application: `callee(arguments...)`. The callee is a value whose
    /// type is a function type, however it was produced.
    Call {
        callee: Box<Expression>,
        callee_span: Span,
        arguments: Vec<Expression>,
        span: Span,
    },
    /// A call whose callee is a bound function named only by its family: the
    /// receiver's type head selects the one declaration. The linker holds no
    /// type facts, so it writes the family alone — the namespace a qualified
    /// path named, the written path, and no more; the check resolves the
    /// family while typing the call and rewrites this into the reference the
    /// callee becomes. A family names no declaration before then, so this
    /// node carries no edge.
    Method {
        /// The namespace a qualified family path named. `None` for a bare
        /// method name, which every namespace's method pool answers.
        namespace: Option<NamespaceId>,
        /// The written path the family was named by: one segment for a method
        /// call, the whole written path for a qualified one.
        path: Vec<String>,
        span: Span,
    },
    Field {
        target: Box<Expression>,
        name: String,
        name_span: Span,
        span: Span,
    },
    Binary {
        op: BinaryOp,
        left: Box<Expression>,
        right: Box<Expression>,
        span: Span,
    },
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
        /// Whether the arms and the default cover every value the subject can
        /// take. The check pass writes it where it reads the subject's type,
        /// and the flow pass reads it to know whether the term can fall
        /// through.
        exhaustive: bool,
        span: Span,
    },
    /// A function body: a named function's value, or the anonymous function a
    /// closure literal declares. Only a holding declaration's value carries
    /// this form; a body that writes a literal holds a reference to it.
    Function(Function),
}

impl Expression {
    /// The shape of value literal this expression writes, when it writes one.
    /// A literal with no type of its own takes its type from the position it
    /// is checked in; the shape says which literal form it is, so a checker
    /// reads this fact rather than matching the expression form again.
    pub(crate) fn shape(&self) -> Option<Shape> {
        match self {
            Expression::Text { .. }
            | Expression::Bool { .. }
            | Expression::Int { .. }
            | Expression::Interpolated { .. } => Some(Shape::Word),
            Expression::Map { .. } => Some(Shape::Record),
            Expression::Variant { .. } => Some(Shape::Variant),
            Expression::Function(_) => Some(Shape::Callable),
            Expression::Reference { .. }
            | Expression::Call { .. }
            | Expression::Method { .. }
            | Expression::Field { .. }
            | Expression::Binary { .. }
            | Expression::Unary { .. }
            | Expression::Match { .. } => None,
        }
    }

    pub(crate) fn span(&self) -> Span {
        match self {
            Expression::Text { span, .. }
            | Expression::Map { span, .. }
            | Expression::Bool { span, .. }
            | Expression::Int { span, .. }
            | Expression::Variant { span, .. }
            | Expression::Interpolated { span, .. }
            | Expression::Reference { span, .. }
            | Expression::Call { span, .. }
            | Expression::Method { span, .. }
            | Expression::Field { span, .. }
            | Expression::Binary { span, .. }
            | Expression::Unary { span, .. }
            | Expression::Match { span, .. } => *span,
            Expression::Function(function) => function.span,
        }
    }
}

/// One part of an interpolated string.
#[derive(Debug)]
pub(crate) enum StringPart {
    /// Literal text.
    Literal(String),
    /// An `@(…)` expression whose text is inserted.
    Expression(Expression),
}

/// One `key = expression` entry of a record literal.
#[derive(Debug)]
pub(crate) struct Field {
    pub(crate) name: String,
    pub(crate) name_span: Span,
    pub(crate) value: Expression,
    pub(crate) span: Span,
}

/// A linked statement.
#[derive(Debug)]
pub(crate) enum Statement {
    /// A local `txt` or `rec` binding and its initializer.
    Bind {
        declaration: DeclarationId,
        value: Expression,
        span: Span,
    },
    Assign {
        declaration: DeclarationId,
        name_span: Span,
        value: Expression,
        span: Span,
    },
    /// `name.field = expression;` replaces one field of a map.
    FieldAssign {
        declaration: DeclarationId,
        name_span: Span,
        field: String,
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
    /// The keyword statement `async <call>;`, which spawns the call.
    Async {
        call: Expression,
        span: Span,
    },
    /// The keyword statement `wait;`, which joins the calling thread's asyncs.
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

impl Statement {
    /// The span one statement covers. A bare expression statement has no span
    /// of its own, so it covers the expression it discards.
    pub(crate) fn span(&self) -> Span {
        match self {
            Statement::Expression(expression) => expression.span(),
            Statement::Bind { span, .. }
            | Statement::Assign { span, .. }
            | Statement::FieldAssign { span, .. }
            | Statement::Return { span, .. }
            | Statement::Panic { span }
            | Statement::Async { span, .. }
            | Statement::Wait { span }
            | Statement::Forever { span, .. }
            | Statement::Break { span } => *span,
        }
    }
}

/// One `pattern => body;` arm of a `match`.
#[derive(Debug)]
pub(crate) struct MatchArm {
    pub(crate) pattern: Pattern,
    pub(crate) body: MatchBody,
    pub(crate) span: Span,
}

/// A match pattern.
#[derive(Debug)]
pub(crate) enum Pattern {
    /// An equality atom: a literal, a name, a field path, or a payloadless
    /// variant.
    Value(Expression),
    /// `Name.Variant(bind, ...)`: matches one enum variant. `bindings` has one
    /// entry per payload position: the local it binds, or `None` for `_`.
    Variant {
        ty: TypeId,
        index: usize,
        bindings: Vec<Option<DeclarationId>>,
        span: Span,
    },
}

/// The body of a match arm: an expression in expression position, or a block
/// of statements in statement position.
#[derive(Debug)]
pub(crate) enum MatchBody {
    Expression(Box<Expression>),
    Block(Vec<Statement>),
}
