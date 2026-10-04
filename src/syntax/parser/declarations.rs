//! Declaration level grammar: modules, imports, functions, variables, and
//! records.

use crate::syntax::ast::{Binding, Function, Import, ModulePath, Parameter, ValueKind};
use crate::syntax::token::TokenKind;

use super::{ParseError, Parser};

impl Parser {
    /// Parse a `module path;` declaration.
    pub(super) fn parse_module(&mut self) -> Result<ModulePath, ParseError> {
        let start = self.advance().start;
        let (segments, _) = self.parse_path_segments("in a module path")?;
        let span = self.span_through_semicolon(start, "after the module path")?;
        Ok(ModulePath { segments, span })
    }

    /// Parse an `import "path";` declaration.
    pub(super) fn parse_import(&mut self) -> Result<Import, ParseError> {
        let start = self.advance().start;
        let (path, _) = self.expect_text("after `import`")?;
        let span = self.span_through_semicolon(start, "after the import path")?;
        Ok(Import { path, span })
    }

    /// Parse a `fn name(parameters) -> kind? { ... };` declaration.
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
        let return_kind = self.parse_return_kind()?;
        let body = self.parse_block()?;
        let span = self.span_through_semicolon(start, "after the function body")?;
        Ok(Function {
            name,
            name_span,
            parameters,
            return_kind,
            body,
            span,
        })
    }

    /// Parse one function parameter: a kind keyword, `txt` or `rec`, then the
    /// name.
    fn parse_parameter(&mut self) -> Result<Parameter, ParseError> {
        let kind = self.parse_value_kind("in the parameter list")?;
        let (name, span) = self.expect_identifier("in the parameter list")?;
        Ok(Parameter { kind, name, span })
    }

    /// Parse an optional `-> txt` or `-> rec` return kind after the parameter
    /// list. An absent arrow means the function returns no value.
    fn parse_return_kind(&mut self) -> Result<Option<ValueKind>, ParseError> {
        if !self.eat(&TokenKind::Arrow) {
            return Ok(None);
        }
        Ok(Some(self.parse_value_kind("after `->`")?))
    }

    /// Parse the one kind keyword, `txt` or `rec`. A parameter and a return
    /// type are the two places a kind is written.
    fn parse_value_kind(&mut self, context: &str) -> Result<ValueKind, ParseError> {
        match self.current().kind {
            TokenKind::Txt => {
                self.advance();
                Ok(ValueKind::Text)
            }
            TokenKind::Rec => {
                self.advance();
                Ok(ValueKind::Record)
            }
            _ => Err(self.error_here(format!(
                "expected `txt` or `rec` {context}, found {}",
                self.current().kind.describe()
            ))),
        }
    }

    /// Parse a `txt name = expression;` or `rec name = expression;`
    /// declaration or statement. The expression is a record literal, a record
    /// variable, or a call returning a value of the declared kind.
    pub(super) fn parse_binding(&mut self, kind: ValueKind) -> Result<Binding, ParseError> {
        let start = self.advance().start;
        let (name, name_span) = self.expect_identifier("in a binding")?;
        self.expect(&TokenKind::Equals, "after the binding name")?;
        let value = self.parse_expression()?;
        let span = self.span_through_semicolon(start, "after the initializer")?;
        Ok(Binding {
            kind,
            name,
            name_span,
            value,
            span,
        })
    }
}
