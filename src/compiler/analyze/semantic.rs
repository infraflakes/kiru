//! The roles of the names one source file writes.
//!
//! A semantic token is a written name and the role the model gives it. The
//! lexer supplies every token's span and the roles of the words that are not
//! names: keywords, literals, operators, comments. The model supplies the
//! roles of the names: declarations, references, written types, use
//! deliveries, fields, and assignments.
//!
//! The roles are the legend's order. A role's slot is its place in the
//! `tokenTypes` array a client reads, so the enumeration and the legend move
//! together.

use std::collections::{HashMap, HashSet};

use crate::compiler::compile::load::LoadedProgram;
use crate::compiler::model::{
    DeclarationId, DeclarationKind, Expression, FileId, Function, MatchBody, Pattern, Program,
    Statement, TypeKind, UseTarget, Visitor, walk_expression, walk_statements,
};
use crate::syntax::{Span, Token, TokenKind, lex_with_comments};

/// The token types a client maps to its theme, in legend order.
pub(crate) const TOKEN_TYPES: &[&str] = &[
    "namespace",
    "type",
    "function",
    "method",
    "property",
    "enumMember",
    "variable",
    "parameter",
    "keyword",
    "string",
    "number",
    "operator",
    "comment",
];

/// The token modifiers a client maps to its theme, in legend order.
pub(crate) const TOKEN_MODIFIERS: &[&str] = &["declaration"];

/// The role one written name plays.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SemanticRole {
    Namespace = 0,
    Type = 1,
    Function = 2,
    Method = 3,
    Property = 4,
    EnumMember = 5,
    Variable = 6,
    Parameter = 7,
    Keyword = 8,
    String = 9,
    Number = 10,
    Operator = 11,
    Comment = 12,
}

impl SemanticRole {
    /// The role's place in the legend.
    pub(crate) fn slot(self) -> usize {
        self as usize
    }
}

/// One written span and the role it plays.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct SemanticToken {
    pub(crate) span: Span,
    pub(crate) role: SemanticRole,
    /// Whether the span is where the name is declared.
    pub(crate) declaration: bool,
}

/// The tokens of one file, in source order.
pub(crate) fn tokens(
    loaded: &LoadedProgram,
    program: &Program,
    file: FileId,
) -> Vec<SemanticToken> {
    let source = &loaded.files[file.0].source;
    let Ok((lexed, comments)) = lex_with_comments(source) else {
        return Vec::new();
    };
    let mut written = Written::new(source, file);
    written.walk(program);
    let (mut facts, paths) = written.facts(loaded, program);
    facts.extend(comments.into_iter().map(|span| Named {
        span,
        role: SemanticRole::Comment,
        declaration: false,
    }));
    let mut tokens: Vec<SemanticToken> = facts
        .iter()
        .filter(|fact| !fact.span.start.eq(&fact.span.end))
        .map(|fact| SemanticToken {
            span: fact.span,
            role: fact.role,
            declaration: fact.declaration,
        })
        .collect();
    for token in lexed {
        if let Some(role) = role_of_token(&token, &facts, &paths) {
            tokens.push(SemanticToken {
                span: token.span,
                role,
                declaration: false,
            });
        }
    }
    tokens.sort_by_key(|token| (token.span.start, token.span.end));
    // Two facts may name one word: the model's role wins over the lexer's, so
    // a later token that starts inside the one before it is dropped.
    let mut painted: Vec<SemanticToken> = Vec::with_capacity(tokens.len());
    for token in tokens {
        if painted
            .last()
            .is_some_and(|last| token.span.start < last.span.end)
        {
            continue;
        }
        painted.push(token);
    }
    painted
}

/// One named span.
struct Named {
    span: Span,
    role: SemanticRole,
    declaration: bool,
}

/// What a body writes that the model names: the references, assignments,
/// fields and methods, and the parameters their roles depend on.
struct Written<'a> {
    source: &'a str,
    file: FileId,
    references: Vec<(Span, DeclarationId)>,
    writes: Vec<(Span, DeclarationId)>,
    fields: Vec<Span>,
    methods: Vec<Span>,
    parameters: HashSet<DeclarationId>,
    /// The bindings the walked bodies declare: a `let` or a pattern binding.
    /// A local's declaration carries no file of its own, so the walk that met
    /// it is the fact that places it here.
    locals: Vec<DeclarationId>,
    paths: Vec<Span>,
}

impl<'a> Written<'a> {
    fn new(source: &'a str, file: FileId) -> Self {
        Self {
            source,
            file,
            references: Vec::new(),
            writes: Vec::new(),
            fields: Vec::new(),
            methods: Vec::new(),
            parameters: HashSet::new(),
            locals: Vec::new(),
            paths: Vec::new(),
        }
    }

    /// Walk every body the file holds: a named function's body, a module
    /// value's initializer, and every closure literal's anonymous function.
    /// An anonymous function carries the file its literal is written in even
    /// though the file's declaration list does not contain it, and the
    /// enclosing body holds only a reference to it, so each body is walked
    /// once.
    fn walk(&mut self, program: &Program) {
        for node in program.declarations.iter() {
            if node.file != Some(self.file) {
                continue;
            }
            if let Some(function) = node.function() {
                self.parameters(function);
                walk_statements(self, &function.body);
            } else if node.kind.module_value()
                && let Some(value) = node.value()
            {
                walk_expression(self, value);
            }
        }
    }

    fn parameters(&mut self, function: &Function) {
        self.parameters.extend(function.parameters.iter().copied());
    }

    /// The facts the model holds for this file, and the written paths whose
    /// earlier segments are namespaces. A path's own span is a fallback: the
    /// name it ends at carries a fact of its own.
    fn facts(&self, loaded: &LoadedProgram, program: &Program) -> (Vec<Named>, Vec<Span>) {
        let mut facts = Vec::new();
        // A closure literal writes no name: the reference the enclosing body
        // holds marks the literal's span, so it paints nothing and falls back
        // to the lexer's roles like any other punctuation.
        let closure_spans: Vec<Span> = self
            .references
            .iter()
            .filter(|(_, declaration)| program.declaration(*declaration).is_anonymous_function())
            .map(|(span, _)| *span)
            .collect();
        let mut paths = self.paths.clone();
        paths.retain(|path| !closure_spans.contains(path));
        let named = |facts: &mut Vec<Named>, span: Span, role: SemanticRole, declaration: bool| {
            facts.push(Named {
                span,
                role,
                declaration,
            });
        };
        for (index, node) in program.declarations.iter().enumerate() {
            // An anonymous function owns no namespace name, so it has no name
            // fact. Its body's own tokens come from the walk above.
            if node.file != Some(self.file) || node.is_anonymous_function() {
                continue;
            }
            let declaration = DeclarationId(index);
            named(
                &mut facts,
                node.name_span,
                role_of(node.kind, declaration, &self.parameters),
                true,
            );
        }
        // A parameter and a `let` binding are declarations the walk met: they
        // carry no file of their own.
        for declaration in self.parameters.iter().chain(self.locals.iter()).copied() {
            let node = program.declaration(declaration);
            named(
                &mut facts,
                node.name_span,
                role_of(node.kind, declaration, &self.parameters),
                true,
            );
        }
        for type_declaration in program
            .types
            .iter()
            .filter(|type_declaration| type_declaration.file == Some(self.file))
        {
            named(
                &mut facts,
                type_declaration.name_span,
                SemanticRole::Type,
                true,
            );
            match &type_declaration.kind {
                TypeKind::Rec(fields) => {
                    for field in fields {
                        named(&mut facts, field.name_span, SemanticRole::Property, true);
                    }
                }
                TypeKind::Enum(variants) => {
                    for variant in variants {
                        named(
                            &mut facts,
                            variant.name_span,
                            SemanticRole::EnumMember,
                            true,
                        );
                    }
                }
            }
        }
        for used in program
            .type_uses
            .iter()
            .filter(|used| used.file == self.file)
        {
            paths.push(used.path);
            named(&mut facts, used.name, SemanticRole::Type, false);
        }
        let deliveries = deliveries(program);
        for (index, used) in loaded.uses.iter().enumerate() {
            if used.file != self.file.0 {
                continue;
            }
            paths.push(used.path_span);
            let role = deliveries
                .get(&index)
                .map_or(SemanticRole::Namespace, |target| {
                    role_of_target(program, *target, &self.parameters)
                });
            named(&mut facts, used.delivered, role, false);
        }
        for &(span, declaration) in &self.references {
            let node = program.declaration(declaration);
            // The enclosing body's edge to a closure is a reference in the
            // model, but the closure literal writes no name.
            if node.is_anonymous_function() {
                continue;
            }
            named(
                &mut facts,
                written_name(self.source, span),
                role_of(node.kind, declaration, &self.parameters),
                false,
            );
        }
        for &span in &self.methods {
            named(
                &mut facts,
                written_name(self.source, span),
                SemanticRole::Method,
                false,
            );
        }
        for &span in &self.fields {
            named(&mut facts, span, SemanticRole::Property, false);
        }
        for &(span, declaration) in &self.writes {
            let node = program.declaration(declaration);
            named(
                &mut facts,
                span,
                role_of(node.kind, declaration, &self.parameters),
                false,
            );
        }
        (facts, paths)
    }
}

impl Visitor for Written<'_> {
    fn reference(&mut self, expression: &Expression) {
        let Expression::Reference { declaration, span } = expression else {
            unreachable!("the walk names a reference");
        };
        self.references.push((*span, *declaration));
        self.paths.push(*span);
    }

    fn method(&mut self, expression: &Expression) {
        let Expression::Method { span, .. } = expression else {
            unreachable!("the walk names a method family");
        };
        self.methods.push(*span);
        self.paths.push(*span);
    }

    fn field(&mut self, expression: &Expression) {
        let Expression::Field { name_span, .. } = expression else {
            unreachable!("the walk names a field");
        };
        self.fields.push(*name_span);
    }

    fn map(&mut self, expression: &Expression) {
        let Expression::Map { fields, .. } = expression else {
            unreachable!("the walk names a map");
        };
        self.fields
            .extend(fields.iter().map(|field| field.name_span));
    }

    fn assign(&mut self, statement: &Statement) {
        let Statement::Assign {
            declaration,
            name_span,
            ..
        } = statement
        else {
            unreachable!("the walk names an assignment");
        };
        self.writes.push((*name_span, *declaration));
    }

    fn field_assign(&mut self, statement: &Statement) {
        // The statement's span names the binding whose field it changes; the
        // field's own name is written without a span of its own.
        let Statement::FieldAssign {
            declaration,
            name_span,
            ..
        } = statement
        else {
            unreachable!("the walk names a field assignment");
        };
        self.writes.push((*name_span, *declaration));
    }

    fn function(&mut self, expression: &Expression) {
        let Expression::Function(function) = expression else {
            unreachable!("the walk names a function");
        };
        self.parameters(function);
    }

    // Every other kind names nothing of its own; its children carry the walk.
    fn text(&mut self, _: &Expression) {}
    fn boolean(&mut self, _: &Expression) {}
    fn integer(&mut self, _: &Expression) {}
    fn variant(&mut self, _: &Expression) {}
    fn interpolated(&mut self, _: &Expression) {}
    fn call(&mut self, _: &Expression) {}
    fn binary(&mut self, _: &Expression) {}
    fn unary(&mut self, _: &Expression) {}
    fn match_expression(&mut self, _: &Expression) {}
    fn bind(&mut self, statement: &Statement) {
        let Statement::Bind { declaration, .. } = statement else {
            unreachable!("the walk names a binding statement");
        };
        self.locals.push(*declaration);
    }

    /// A match pattern binding is a local the body declares.
    fn variant_pattern(&mut self, pattern: &Pattern) {
        let Pattern::Variant { bindings, .. } = pattern else {
            unreachable!("the walk names a variant pattern");
        };
        self.locals.extend(bindings.iter().flatten().copied());
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

/// The role a declaration plays where it stands, or where a reference names
/// it. A local that is a function's parameter is a parameter wherever it is
/// written.
fn role_of(
    kind: DeclarationKind,
    declaration: DeclarationId,
    parameters: &HashSet<DeclarationId>,
) -> SemanticRole {
    match kind {
        DeclarationKind::Function | DeclarationKind::Native => SemanticRole::Function,
        // A member an impl block filed under its type is reached through the
        // type, never by a namespace-level name.
        DeclarationKind::Impl => SemanticRole::Method,
        DeclarationKind::ModuleValue => SemanticRole::Variable,
        DeclarationKind::Local if parameters.contains(&declaration) => SemanticRole::Parameter,
        DeclarationKind::Local => SemanticRole::Variable,
        DeclarationKind::Rec | DeclarationKind::Enum => SemanticRole::Type,
    }
}

/// The role a `use` delivery plays: the namespace whose qualifier it brings,
/// or the role of the declaration it names.
fn role_of_target(
    program: &Program,
    target: UseTarget,
    parameters: &HashSet<DeclarationId>,
) -> SemanticRole {
    match target {
        UseTarget::Namespace(_) => SemanticRole::Namespace,
        UseTarget::Type(_) => SemanticRole::Type,
        UseTarget::Function(declaration) => {
            let node = program.declaration(declaration);
            role_of(node.kind, declaration, parameters)
        }
    }
}

/// Every `use` delivery in the program, keyed by the item that wrote it.
fn deliveries(program: &Program) -> HashMap<usize, UseTarget> {
    let mut deliveries = HashMap::new();
    for namespace in &program.namespaces {
        for entries in namespace.brought.values() {
            for delivery in entries {
                deliveries.insert(delivery.use_index, delivery.target);
            }
        }
    }
    deliveries
}

/// The span of the name a written path ends at: the text after its last `::`.
fn written_name(source: &str, span: Span) -> Span {
    let text = &source[span.start..span.end];
    let start = text.rfind("::").map_or(0, |separator| separator + 2);
    Span::new(span.start + start, span.end)
}

/// The role a token plays when no name fact covers it.
fn role_of_token(token: &Token, facts: &[Named], paths: &[Span]) -> Option<SemanticRole> {
    if let Some(fact) = facts.iter().find(|fact| fact.span.covers(token.span)) {
        return Some(fact.role);
    }
    if paths.iter().any(|path| path.covers(token.span)) {
        return Some(SemanticRole::Namespace);
    }
    match token.kind {
        TokenKind::Ident => None,
        TokenKind::Text(_) => Some(SemanticRole::String),
        TokenKind::Int(_) => Some(SemanticRole::Number),
        TokenKind::Use
        | TokenKind::Pub
        | TokenKind::Fn
        | TokenKind::Impl
        | TokenKind::Enum
        | TokenKind::Let
        | TokenKind::Rec
        | TokenKind::Match
        | TokenKind::Return
        | TokenKind::Panic
        | TokenKind::Async
        | TokenKind::Wait
        | TokenKind::For
        | TokenKind::Break
        | TokenKind::True
        | TokenKind::False => Some(SemanticRole::Keyword),
        TokenKind::Eof => None,
        TokenKind::Wildcard
        | TokenKind::InterpolationStart
        | TokenKind::InterpolationEnd
        | TokenKind::PathSep
        | TokenKind::Dot
        | TokenKind::Colon
        | TokenKind::Comma
        | TokenKind::Semi
        | TokenKind::Equals
        | TokenKind::EqEq
        | TokenKind::NotEq
        | TokenKind::Plus
        | TokenKind::Minus
        | TokenKind::Star
        | TokenKind::Slash
        | TokenKind::Percent
        | TokenKind::Bang
        | TokenKind::AndAnd
        | TokenKind::OrOr
        | TokenKind::Arrow
        | TokenKind::FatArrow
        | TokenKind::Less
        | TokenKind::LessEq
        | TokenKind::Greater
        | TokenKind::GreaterEq
        | TokenKind::LBrace
        | TokenKind::RBrace
        | TokenKind::LParen
        | TokenKind::RParen
        | TokenKind::LBracket
        | TokenKind::RBracket => Some(SemanticRole::Operator),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::compiler::build;
    use crate::compiler::test_support::VirtualSource;
    use std::path::Path;

    /// The tokens of a one-file program, as (role, written text) pairs.
    fn painted(source: &str) -> Vec<(String, String)> {
        let tree = VirtualSource::new(&[("main.kiru", source)]);
        let project = crate::compiler::test_support::virtual_project(tree.root());
        let built = build(&project, &tree).expect("the program checks");
        let file = built
            .loaded
            .files
            .iter()
            .position(|file| file.path == Path::new("/virtual/main.kiru"))
            .expect("the program's file is loaded");
        tokens(&built.loaded, &built.program, FileId(file))
            .into_iter()
            .map(|token| {
                (
                    format!(
                        "{}{}",
                        TOKEN_TYPES[token.role.slot()],
                        if token.declaration { "*" } else { "" }
                    ),
                    source[token.span.start..token.span.end].to_owned(),
                )
            })
            .collect()
    }

    #[test]
    fn the_names_of_a_file_carry_their_roles() {
        let source = "use std::print;\nrec Point { x: int, y: int };\nenum Shape { Dot };\nimpl Point { pub fn at(self: Point, index: int) -> int { return index; }; };\nfn speak(prefix: txt) -> txt { return prefix; };\nfn main() {\n    let point: Point = { x = 1, y = 2 };\n    let count: int = point.at(0);\n    point.x = count;\n    print(speak(\"hello\"));\n};\n";
        let painted = painted(source);
        for expected in [
            ("keyword", "use"),
            ("namespace", "std"),
            ("function", "print"),
            ("type*", "Point"),
            ("property*", "x"),
            ("type", "int"),
            ("type*", "Shape"),
            ("enumMember*", "Dot"),
            ("type", "Point"),
            ("method*", "at"),
            ("parameter", "index"),
            ("parameter", "prefix"),
            ("function*", "speak"),
            ("function*", "main"),
            ("type", "txt"),
            ("variable*", "point"),
            ("variable*", "count"),
            ("parameter*", "index"),
            ("parameter*", "self"),
            ("method", "at"),
            ("property", "x"),
            ("number", "0"),
            ("string", "\"hello\""),
        ] {
            assert!(
                painted.contains(&(expected.0.to_owned(), expected.1.to_owned())),
                "expected {expected:?} among {painted:?}"
            );
        }
        assert!(
            !painted
                .iter()
                .any(|(role, text)| role == "namespace" && text == "int"),
            "a written type is no namespace: {painted:?}"
        );
    }
}
