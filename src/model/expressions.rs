//! The linked expressions and statements. Every name is already an edge to a
//! declaration node.

use crate::syntax::Span;

use super::ids::DeclarationId;

/// A linked expression. Every name is already an edge to a node.
#[derive(Debug)]
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
    /// A reference to a declaration or a binding. The leading `::`, if any,
    /// was consumed by resolution and is not part of the edge.
    Reference {
        declaration: DeclarationId,
        span: Span,
    },
    Call {
        callee: DeclarationId,
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
            | Expression::Reference { span, .. }
            | Expression::Call { span, .. }
            | Expression::Field { span, .. }
            | Expression::Add { span, .. } => *span,
        }
    }
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
    /// `name.field = expression;` replaces one field of a mutable record.
    FieldAssign {
        declaration: DeclarationId,
        name_span: Span,
        field: String,
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
    /// The keyword statement `async <call>;`, which spawns the call.
    Async {
        call: Expression,
        span: Span,
    },
    /// The keyword statement `wait;`, which joins the calling thread's asyncs.
    Wait {
        span: Span,
    },
    Switch {
        subject: Expression,
        cases: Vec<Case>,
        default: Option<Vec<Statement>>,
        span: Span,
    },
    /// `for item in <list> { ... };` runs the body once per element, with the
    /// loop binding `item` bound to each text element in turn.
    ForEach {
        item: DeclarationId,
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
            | Statement::Switch { span, .. }
            | Statement::ForEach { span, .. }
            | Statement::Forever { span, .. }
            | Statement::Break { span } => *span,
        }
    }
}

/// One `case(pattern) { ... };` arm of a switch.
#[derive(Debug)]
pub(crate) struct Case {
    pub(crate) pattern: Expression,
    pub(crate) body: Vec<Statement>,
    pub(crate) span: Span,
}
