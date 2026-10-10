//! Declaration level grammar: modules, imports, functions, variables, and
//! records.

use crate::syntax::ast::{Binding, Declaration, Function, Import, Module, Parameter};
use crate::syntax::token::TokenKind;
use crate::types::Type;

use super::{ParseError, Parser};

impl Parser {
    /// Parse a `mod a::b { declarations }` block: an inline namespace. A file
    /// may hold several, and reopening a path merges into the same namespace.
    pub(super) fn parse_mod(&mut self) -> Result<Module, ParseError> {
        let start = self.advance().start;
        let (path, _) = self.parse_path_segments("in a module path")?;
        self.enter_nesting()?;
        self.expect(&TokenKind::LBrace, "to open the module")?;
        let mut declarations = Vec::new();
        while !self.check(&TokenKind::RBrace) {
            if self.check(&TokenKind::Eof) {
                return Err(
                    self.error_here("expected `}` to close the module, found the end of the file")
                );
            }
            declarations.push(self.parse_declaration()?);
        }
        self.expect(&TokenKind::RBrace, "to close the module")?;
        self.leave_nesting();
        let span = self.span_through_semicolon(start, "after the module")?;
        Ok(Module {
            path,
            declarations,
            span,
        })
    }

    /// Parse one declaration: a function or a module value. A module value may
    /// not be `mut`.
    pub(super) fn parse_declaration(&mut self) -> Result<Declaration, ParseError> {
        match &self.current().kind {
            TokenKind::Fn => Ok(Declaration::Function(self.parse_function()?)),
            TokenKind::Let => {
                let binding = self.parse_let_binding()?;
                if binding.mutable {
                    return Err(ParseError {
                        span: binding.name_span,
                        message: "a module value cannot be `mut`".to_owned(),
                    });
                }
                Ok(Declaration::Binding(binding))
            }
            other => Err(self.error_here(format!(
                "expected a declaration, found {}",
                other.describe()
            ))),
        }
    }

    /// Parse an `import "path";` declaration.
    pub(super) fn parse_import(&mut self) -> Result<Import, ParseError> {
        let start = self.advance().start;
        let (path, _) = self.expect_text("after `import`")?;
        let span = self.span_through_semicolon(start, "after the import path")?;
        Ok(Import { path, span })
    }

    /// Parse a `fn name(parameters) -> type? { ... };` declaration.
    pub(super) fn parse_function(&mut self) -> Result<Function, ParseError> {
        let start = self.advance().start;
        let (name, name_span) = self.expect_identifier("in a function declaration")?;
        let (parameters, _) = self.parse_delimited(
            &TokenKind::LParen,
            &TokenKind::RParen,
            "after the function name",
            "after the parameter list",
            Self::parse_parameter,
        )?;
        let return_type = self.parse_return_type()?;
        let body = self.parse_block()?;
        let span = self.span_through_semicolon(start, "after the function body")?;
        Ok(Function {
            name,
            name_span,
            parameters,
            return_type,
            body,
            span,
        })
    }

    /// Parse one function parameter: an optional `mut`, the name, then a
    /// `<type>` annotation.
    fn parse_parameter(&mut self) -> Result<Parameter, ParseError> {
        let mutable = self.eat(&TokenKind::Mut);
        let (name, span) = self.expect_identifier("in the parameter list")?;
        let ty = self.parse_type_annotation("in the parameter list")?;
        Ok(Parameter {
            ty,
            mutable,
            name,
            span,
        })
    }

    /// Parse an optional `-> type` return type after the parameter list. An
    /// absent arrow means the function returns no value.
    fn parse_return_type(&mut self) -> Result<Option<Type>, ParseError> {
        if !self.eat(&TokenKind::Arrow) {
            return Ok(None);
        }
        Ok(Some(self.parse_type("after `->`")?))
    }

    /// Parse one type keyword, as declared in the type registry.
    pub(super) fn parse_type(&mut self, context: &str) -> Result<Type, ParseError> {
        match self.current().kind {
            TokenKind::Type(ty) => {
                self.advance();
                Ok(ty)
            }
            _ => Err(self.error_here(format!(
                "expected {} {context}, found {}",
                written_types(),
                self.current().kind.describe()
            ))),
        }
    }

    /// Parse a `<type>` annotation, as on a binding or a parameter.
    fn parse_type_annotation(&mut self, context: &str) -> Result<Type, ParseError> {
        self.expect(&TokenKind::Less, context)?;
        let ty = self.parse_type(context)?;
        self.expect(&TokenKind::Greater, context)?;
        Ok(ty)
    }

    /// Parse a `let [mut] name<type> = expression;` declaration or statement.
    pub(super) fn parse_let_binding(&mut self) -> Result<Binding, ParseError> {
        let start = self.advance().start;
        let mutable = self.eat(&TokenKind::Mut);
        self.parse_binding_tail(mutable, start)
    }

    /// Parse the name, `<type>` annotation, and initializer of a binding. The
    /// `let` and any `mut` are already consumed, and `start` is where the
    /// binding began.
    fn parse_binding_tail(&mut self, mutable: bool, start: usize) -> Result<Binding, ParseError> {
        let (name, name_span) = self.expect_identifier("in a binding")?;
        let ty = self.parse_type_annotation("in a binding")?;
        self.expect(&TokenKind::Equals, "after the binding type")?;
        let value = self.parse_expression()?;
        let span = self.span_through_semicolon(start, "after the initializer")?;
        Ok(Binding {
            ty,
            mutable,
            name,
            name_span,
            value,
            span,
        })
    }
}

/// The written type keywords, for a diagnostic. Generated from the registry so
/// it can never drift from the type set.
fn written_types() -> String {
    let keywords: Vec<String> = Type::ALL
        .iter()
        .filter_map(|ty| ty.keyword())
        .map(|keyword| format!("`{keyword}`"))
        .collect();
    match keywords.as_slice() {
        [] => "a type".to_owned(),
        [only] => only.clone(),
        [rest @ .., last] => format!("{} or {last}", rest.join(", ")),
    }
}
