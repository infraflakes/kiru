//! The syntax tree produced by the parser.
//!
//! A `File` is one loaded source file: an optional module path, its imports,
//! and its declarations. Statements exist only inside function, `defer`, and
//! `case` bodies.

use crate::syntax::Span;

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct File {
    pub(crate) module: Option<ModulePath>,
    pub(crate) imports: Vec<Import>,
    pub(crate) declarations: Vec<Declaration>,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct ModulePath {
    pub(crate) segments: Vec<String>,
    pub(crate) span: Span,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct Import {
    pub(crate) path: String,
    pub(crate) span: Span,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) enum Declaration {
    Function(Function),
    Text(TextBinding),
    Rec(RecBinding),
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct Function {
    pub(crate) name: String,
    pub(crate) name_span: Span,
    pub(crate) parameters: Vec<Parameter>,
    pub(crate) body: Vec<Statement>,
    pub(crate) span: Span,
}

/// The kind a parameter declares. A parameter is the only place a kind is
/// written down.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) enum ParameterKind {
    /// The `txt` keyword: text data.
    Text,
    /// The `rec` keyword: a record of text.
    Record,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct Parameter {
    pub(crate) kind: ParameterKind,
    pub(crate) name: String,
    pub(crate) span: Span,
}

/// A `txt name = expression;` declaration or statement.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct TextBinding {
    pub(crate) name: String,
    pub(crate) name_span: Span,
    pub(crate) value: Expression,
    pub(crate) span: Span,
}

/// A `rec name = expression;` declaration or statement. The expression is a
/// record literal, a record variable, or a call returning a record.
#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct RecBinding {
    pub(crate) name: String,
    pub(crate) name_span: Span,
    pub(crate) value: Expression,
    pub(crate) span: Span,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct Field {
    pub(crate) name: String,
    pub(crate) name_span: Span,
    pub(crate) value: Expression,
    pub(crate) span: Span,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) enum Statement {
    Text(TextBinding),
    Rec(RecBinding),
    Assignment {
        name: String,
        name_span: Span,
        value: Expression,
        span: Span,
    },
    Expression(Expression),
    /// An early exit. A value return carries text or record; a valueless
    /// return ends a function that returns nothing.
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
    Defer {
        body: Vec<Statement>,
        span: Span,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) struct Case {
    pub(crate) pattern: Expression,
    pub(crate) body: Vec<Statement>,
    pub(crate) span: Span,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) enum Expression {
    Text {
        value: String,
        span: Span,
    },
    Record {
        fields: Vec<Field>,
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
            | Expression::Name { span, .. }
            | Expression::Call { span, .. }
            | Expression::Field { span, .. }
            | Expression::Add { span, .. } => *span,
        }
    }
}
