//! Declaration level grammar: modules, imports, functions, variables, and
//! records.

use crate::syntax::ast::{
    Function, Import, ModulePath, Parameter, ParameterKind, RecBinding, TextBinding,
};
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

    /// Parse a `fn name(parameters) { ... };` declaration.
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
        let body = self.parse_block()?;
        let span = self.span_through_semicolon(start, "after the function body")?;
        Ok(Function {
            name,
            name_span,
            parameters,
            body,
            span,
        })
    }

    /// Parse one function parameter: a kind keyword, `txt` or `rec`, then the
    /// name. A parameter is the only place a kind is written.
    fn parse_parameter(&mut self) -> Result<Parameter, ParseError> {
        let kind = match self.current().kind {
            TokenKind::Txt => {
                self.advance();
                ParameterKind::Text
            }
            TokenKind::Rec => {
                self.advance();
                ParameterKind::Record
            }
            _ => {
                return Err(self.error_here(format!(
                    "expected `txt` or `rec` in the parameter list, found {}",
                    self.current().kind.describe()
                )));
            }
        };
        let (name, span) = self.expect_identifier("in the parameter list")?;
        Ok(Parameter { kind, name, span })
    }

    /// Parse a `txt name = expression;` declaration or statement.
    pub(super) fn parse_text_binding(&mut self) -> Result<TextBinding, ParseError> {
        let start = self.advance().start;
        let (name, name_span) = self.expect_identifier("in a text binding")?;
        self.expect(&TokenKind::Equals, "after the binding name")?;
        let value = self.parse_expression()?;
        let span = self.span_through_semicolon(start, "after the initializer")?;
        Ok(TextBinding {
            name,
            name_span,
            value,
            span,
        })
    }

    /// Parse a `rec name = expression;` declaration or statement. The
    /// expression is a record literal, a record variable, or a call returning
    /// a record.
    pub(super) fn parse_rec_binding(&mut self) -> Result<RecBinding, ParseError> {
        let start = self.advance().start;
        let (name, name_span) = self.expect_identifier("in a record binding")?;
        self.expect(&TokenKind::Equals, "after the binding name")?;
        let value = self.parse_expression()?;
        let span = self.span_through_semicolon(start, "after the initializer")?;
        Ok(RecBinding {
            name,
            name_span,
            value,
            span,
        })
    }
}
