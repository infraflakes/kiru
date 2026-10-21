//! The reads a checked program records.
//!
//! After the check, every name the program reads is an edge: a
//! `Reference { declaration }` under the expression it reads. This walk
//! collects those edges and the bindings they can name — a parameter, a
//! `let`, a match-pattern binding — so the unused-binding warning answers its
//! question once. The walk reads the model through the one structural
//! visitor, so a new node kind fails to compile until this walk names it.

use std::collections::HashSet;

use crate::compiler::model::{
    DeclarationId, Expression, FileId, Function, MatchBody, Pattern, Program, Statement, Visitor,
    walk_expression, walk_statements,
};

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
    pub(crate) bindings: Vec<Bound>,
}

/// Collect the reads and bindings of the bodies in the program, or of the
/// bodies one file holds. A body whose declaration has no file is a native,
/// which holds no body; a local's file is the file of the body it is written
/// in, which the walk carries. A file-scoped walk leaves the other files'
/// declarations alone: an analysis links only the file it checks.
pub(crate) fn collect_reads(program: &Program, only: Option<FileId>) -> Reads {
    let mut reads = Reads::default();
    for declaration in &program.declarations {
        let Some(file) = declaration.file else {
            continue;
        };
        if only.is_some_and(|wanted| wanted != file) {
            continue;
        }
        let mut collector = Collector {
            reads: &mut reads,
            file,
        };
        match declaration.function() {
            Some(function) => {
                collector.parameters(function);
                walk_statements(&mut collector, &function.body);
            }
            None if declaration.kind.module_value() => {
                let Some(value) = declaration.value() else {
                    continue;
                };
                walk_expression(&mut collector, value);
            }
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
        let Expression::Reference { declaration, .. } = expression else {
            unreachable!("the walk names a reference");
        };
        self.reads.referenced.insert(*declaration);
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

    // Every other kind carries no binding; its children carry the walk.
    fn text(&mut self, _: &Expression) {}
    fn map(&mut self, _: &Expression) {}
    fn boolean(&mut self, _: &Expression) {}
    fn integer(&mut self, _: &Expression) {}
    fn variant(&mut self, _: &Expression) {}
    fn interpolated(&mut self, _: &Expression) {}
    fn call(&mut self, _: &Expression) {}
    fn method(&mut self, _: &Expression) {}
    fn field(&mut self, _: &Expression) {}
    fn binary(&mut self, _: &Expression) {}
    fn unary(&mut self, _: &Expression) {}
    fn match_expression(&mut self, _: &Expression) {}
    fn assign(&mut self, _: &Statement) {}
    fn field_assign(&mut self, _: &Statement) {}
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
