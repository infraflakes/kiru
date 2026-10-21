//! The reads a checked program records.
//!
//! After the check, every name the program reads is an edge: a
//! `Reference { declaration }` under the expression it reads. This walk
//! collects those edges and the bindings they can name — a parameter, a
//! `let`, a match-pattern binding — so the facts that already exist answer the
//! questions about them once: whether a local is ever read, and which
//! declaration a position in a file stands on. The writes it records beside
//! them are the edges an assignment writes the name by, and the fields are
//! the names a field access reads by; both carry no read edge, so they ride
//! the same walk instead of a second one. The walk reads the model
//! through the one structural visitor, so a new node kind fails to compile
//! until this walk names it.

use std::collections::HashSet;

use crate::compiler::model::{
    DeclarationId, Expression, FileId, Function, MatchBody, Pattern, Program, Statement, Visitor,
    walk_expression, walk_statements,
};
use crate::syntax::Span;

/// One read the program records: the declaration it names, the file the read
/// is written in, and the span of the written name.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Read {
    pub(crate) declaration: DeclarationId,
    pub(crate) file: FileId,
    pub(crate) span: Span,
}

impl Read {
    /// The span of the name the read names: a read may be written as a path,
    /// and the path ends at the name, so the name is the text after the last
    /// path separator the path wrote.
    pub(crate) fn name_span(self, source: &str) -> Span {
        let path = &source[self.span.start..self.span.end];
        let name = path.rfind("::").map(|separator| separator + 2).unwrap_or(0);
        Span::new(self.span.start + name, self.span.end)
    }
}

/// One write the program records: the binding an assignment names, the file
/// it is written in, and the span of the written name. A write is not a read:
/// a binding only assigned is still never read.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Write {
    pub(crate) declaration: DeclarationId,
    pub(crate) file: FileId,
    pub(crate) span: Span,
}

/// One field access the program records: the name the access reads by, and
/// the file it is written in.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Property {
    pub(crate) file: FileId,
    pub(crate) span: Span,
}

/// One binding a body declares: a parameter, a `let`, or a match-pattern
/// binding, with the file the body is written in.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Bound {
    pub(crate) declaration: DeclarationId,
    pub(crate) file: FileId,
}

/// The reads and bindings of a whole program.
#[derive(Debug, Default)]
pub(crate) struct Reads {
    /// The declarations some reference names. A name in here was read.
    pub(crate) referenced: HashSet<DeclarationId>,
    pub(crate) reads: Vec<Read>,
    pub(crate) writes: Vec<Write>,
    pub(crate) properties: Vec<Property>,
    pub(crate) bindings: Vec<Bound>,
}

/// Collect the reads and bindings of every body in the program. A body whose
/// declaration has no file is a native, which holds no body; a local's file is
/// the file of the body it is written in, which the walk carries.
pub(crate) fn collect_reads(program: &Program) -> Reads {
    let mut reads = Reads::default();
    for declaration in &program.declarations {
        let Some(file) = declaration.file else {
            continue;
        };
        let mut collector = Collector {
            reads: &mut reads,
            file,
        };
        match declaration.function() {
            Some(function) => {
                collector.parameters(function);
                walk_statements(&mut collector, &function.body);
            }
            None if declaration.kind.module_value() => walk_expression(
                &mut collector,
                declaration
                    .value()
                    .expect("a module value carries its initializer"),
            ),
            None => {}
        }
    }
    reads
}

/// The walk over one body: it records the bindings the body declares and
/// every read, keyed by the file the body is written in. A closure inside the
/// body is the body's code, so the walk descends into it.
struct Collector<'a> {
    reads: &'a mut Reads,
    file: FileId,
}

impl Collector<'_> {
    /// The declaration a binding statement declares, or a panic when the walk
    /// hands over a statement that carries no declaration.
    fn binding(&mut self, statement: &Statement) {
        let Statement::Bind { declaration, .. } = statement else {
            unreachable!("the walk names a binding statement");
        };
        self.reads.bindings.push(Bound {
            declaration: *declaration,
            file: self.file,
        });
    }

    /// The parameters a function value owns: closures carry their own.
    fn parameters(&mut self, function: &Function) {
        for parameter in &function.parameters {
            self.reads.bindings.push(Bound {
                declaration: *parameter,
                file: self.file,
            });
        }
    }
}

impl Visitor for Collector<'_> {
    fn reference(&mut self, expression: &Expression) {
        let Expression::Reference {
            declaration, span, ..
        } = expression
        else {
            unreachable!("the walk names a reference");
        };
        self.reads.referenced.insert(*declaration);
        self.reads.reads.push(Read {
            declaration: *declaration,
            file: self.file,
            span: *span,
        });
    }

    fn function(&mut self, expression: &Expression) {
        let Expression::Function(function) = expression else {
            unreachable!("the walk names a function");
        };
        self.parameters(function);
    }

    fn bind(&mut self, statement: &Statement) {
        self.binding(statement);
    }
    fn variant_pattern(&mut self, pattern: &Pattern) {
        let Pattern::Variant { bindings, .. } = pattern else {
            unreachable!("the walk names a variant pattern");
        };
        for binding in bindings.iter().flatten() {
            self.reads.bindings.push(Bound {
                declaration: *binding,
                file: self.file,
            });
        }
    }

    // Every other kind carries no binding and no edge of its own; its
    // children carry the walk.
    fn text(&mut self, _: &Expression) {}
    fn map(&mut self, expression: &Expression) {
        let Expression::Map { fields, .. } = expression else {
            unreachable!("the walk names a map");
        };
        for field in fields {
            self.reads.properties.push(Property {
                file: self.file,
                span: field.name_span,
            });
        }
    }
    fn boolean(&mut self, _: &Expression) {}
    fn integer(&mut self, _: &Expression) {}
    fn variant(&mut self, _: &Expression) {}
    fn interpolated(&mut self, _: &Expression) {}
    fn call(&mut self, _: &Expression) {}
    fn method(&mut self, _: &Expression) {}
    fn field(&mut self, expression: &Expression) {
        let Expression::Field { name_span, .. } = expression else {
            unreachable!("the walk names a field access");
        };
        self.reads.properties.push(Property {
            file: self.file,
            span: *name_span,
        });
    }
    fn binary(&mut self, _: &Expression) {}
    fn unary(&mut self, _: &Expression) {}
    fn match_expression(&mut self, _: &Expression) {}
    fn assign(&mut self, statement: &Statement) {
        let Statement::Assign {
            declaration,
            name_span,
            ..
        } = statement
        else {
            unreachable!("the walk names an assignment");
        };
        self.reads.writes.push(Write {
            declaration: *declaration,
            file: self.file,
            span: *name_span,
        });
    }
    fn field_assign(&mut self, statement: &Statement) {
        let Statement::FieldAssign {
            declaration,
            name_span,
            ..
        } = statement
        else {
            unreachable!("the walk names a field assignment");
        };
        self.reads.writes.push(Write {
            declaration: *declaration,
            file: self.file,
            span: *name_span,
        });
    }
    fn expression_statement(&mut self, _: &Statement) {}
    fn return_statement(&mut self, _: &Statement) {}
    fn panic(&mut self, _: &Statement) {}
    fn asynchronous(&mut self, _: &Statement) {}
    fn wait(&mut self, _: &Statement) {}
    fn forever(&mut self, _: &Statement) {}
    fn break_statement(&mut self, _: &Statement) {}
    fn value_pattern(&mut self, _: &Pattern) {}
    fn expression_body(&mut self, _: &MatchBody) {}
    fn block_body(&mut self, _: &MatchBody) {}
    fn enters_deferred_body(&self) -> bool {
        true
    }
}
