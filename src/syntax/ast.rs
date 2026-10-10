//! The syntax tree produced by the parser.
//!
//! A `File` is one loaded source file: its top-level items in order. An `item`
//! is a declaration, an inline `mod` block, or an `import`. Statements exist
//! only inside function and `match` bodies.

use crate::syntax::Span;
use crate::types::Type;

/// One parsed source file: its top-level items in order.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct File {
    pub(crate) items: Vec<Item>,
}

/// A top-level item: a declaration, an inline `mod` block, or an `import`.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Item {
    Declaration(Declaration),
    Module(Module),
    Import(Import),
}

/// A `mod a::b { declarations }` block: an inline namespace. A file may hold
/// several, and reopening a path merges into the same namespace.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Module {
    pub(crate) path: Vec<String>,
    pub(crate) declarations: Vec<Declaration>,
    pub(crate) span: Span,
}

/// One `import "path";` item. An import is global-level only and is never
/// written inside a `mod` block.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Import {
    pub(crate) path: String,
    pub(crate) span: Span,
}

/// A top-level declaration.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum Declaration {
    Function(Function),
    Binding(Binding),
}

/// A `fn name(parameters) -> type? { body };` declaration. An absent return
/// type means the function returns no value.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Function {
    pub(crate) name: String,
    pub(crate) name_span: Span,
    pub(crate) parameters: Vec<Parameter>,
    pub(crate) return_type: Option<Type>,
    pub(crate) body: Vec<Statement>,
    pub(crate) span: Span,
}

/// One `[mut] name<type>` parameter of a function. A `mut` parameter may be
/// reassigned and have its fields mutated; the caller's value is unaffected.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Parameter {
    pub(crate) ty: Type,
    pub(crate) mutable: bool,
    pub(crate) name: String,
    pub(crate) span: Span,
}

/// A `let [mut] name<type> = expression;` declaration or statement. The
/// initializer is a literal, a name, a field path, or a call returning a value
/// of the declared type.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Binding {
    pub(crate) ty: Type,
    pub(crate) mutable: bool,
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
    /// `name.field = expression;` replaces one field of a mutable record.
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
    /// `for item in <list> { ... };` runs the body once per element, binding
    /// `item` to each text element in turn.
    ForEach {
        item: String,
        item_span: Span,
        iterable: Expression,
        body: Vec<Statement>,
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

/// One `pattern => body;` arm of a match. A `_` arm is the default.
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
    Record {
        fields: Vec<Field>,
        span: Span,
    },
    /// A `[a, b, c]` list literal of text elements.
    List {
        elements: Vec<Expression>,
        span: Span,
    },
    /// A string with `@(…)` interpolation, split into literal and expression
    /// parts. A string with no interpolation is a plain `Text`.
    Interpolated {
        parts: Vec<StringPart>,
        span: Span,
    },
    Name {
        /// An explicit leading `::`, which resolves against the root
        /// namespace only.
        root: bool,
        path: Vec<String>,
        span: Span,
    },
    Call {
        /// An explicit leading `::`, which resolves against the root
        /// namespace only.
        root: bool,
        callee: Vec<String>,
        callee_span: Span,
        arguments: Vec<Expression>,
        span: Span,
    },
    Field {
        target: Box<Expression>,
        name: String,
        name_span: Span,
        span: Span,
    },
    Add {
        left: Box<Expression>,
        right: Box<Expression>,
        span: Span,
    },
    /// `match subject { pattern => body; … };` — a term whose value is the
    /// taken arm's expression, or void when its arms are blocks.
    Match {
        subject: Box<Expression>,
        cases: Vec<MatchArm>,
        default: Option<MatchBody>,
        span: Span,
    },
}

impl Expression {
    pub(crate) fn span(&self) -> Span {
        match self {
            Expression::Text { span, .. }
            | Expression::Record { span, .. }
            | Expression::List { span, .. }
            | Expression::Interpolated { span, .. }
            | Expression::Name { span, .. }
            | Expression::Call { span, .. }
            | Expression::Field { span, .. }
            | Expression::Add { span, .. }
            | Expression::Match { span, .. } => *span,
        }
    }
}

/// One part of an interpolated string.
#[derive(Debug, PartialEq, Eq)]
pub(crate) enum StringPart {
    /// Literal text.
    Literal(String),
    /// An `@(…)` expression whose text is inserted.
    Expression(Expression),
}
