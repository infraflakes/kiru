//! The syntax tree produced by the parser.
//!
//! A `File` is one loaded source file: an optional module path, its imports,
//! and its declarations. Statements exist only inside function, `defer`, and
//! `case` bodies.

use crate::syntax::Span;

/// One parsed source file: its module path, imports, and declarations.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct File {
    pub(crate) module: Option<ModulePath>,
    pub(crate) imports: Vec<Import>,
    pub(crate) declarations: Vec<Declaration>,
}

/// The `module a::b;` path of a file.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct ModulePath {
    pub(crate) segments: Vec<String>,
    pub(crate) span: Span,
}

/// One `import "path";` declaration.
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

/// A `fn name(parameters) -> kind? { body };` declaration. An absent return
/// kind means the function returns no value.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Function {
    pub(crate) name: String,
    pub(crate) name_span: Span,
    pub(crate) parameters: Vec<Parameter>,
    pub(crate) return_kind: Option<ValueKind>,
    pub(crate) body: Vec<Statement>,
    pub(crate) span: Span,
}

/// The kind a `txt` or `rec` keyword declares, on a parameter, a binding, or
/// a function's return type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ValueKind {
    /// The `txt` keyword: text data.
    Text,
    /// The `rec` keyword: a record of text.
    Record,
    /// The `list` keyword: an ordered sequence of text.
    List,
}

/// One `txt`/`rec` parameter of a function. A `mut` parameter may be
/// reassigned and have its fields mutated; the caller's value is unaffected.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Parameter {
    pub(crate) kind: ValueKind,
    pub(crate) mutable: bool,
    pub(crate) name: String,
    pub(crate) span: Span,
}

/// A `[mut] txt name = expression;` or `[mut] rec name = expression;`
/// declaration or statement. The initializer is a record literal, a record
/// variable, or a call returning a value of the declared kind.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Binding {
    pub(crate) kind: ValueKind,
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
    /// An early exit. A value return carries the function's declared kind; a
    /// valueless return ends a function that declares no return kind.
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
    Switch {
        subject: Expression,
        cases: Vec<Case>,
        default: Option<Vec<Statement>>,
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

/// One `case(pattern) { body };` arm of a switch.
#[derive(Debug, PartialEq, Eq)]
pub(crate) struct Case {
    pub(crate) pattern: Expression,
    pub(crate) body: Vec<Statement>,
    pub(crate) span: Span,
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
}

impl Expression {
    pub(crate) fn span(&self) -> Span {
        match self {
            Expression::Text { span, .. }
            | Expression::Record { span, .. }
            | Expression::List { span, .. }
            | Expression::Name { span, .. }
            | Expression::Call { span, .. }
            | Expression::Field { span, .. }
            | Expression::Add { span, .. } => *span,
        }
    }
}
