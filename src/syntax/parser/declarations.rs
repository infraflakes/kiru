//! Declaration level grammar: functions, variables, recs, and nominal type
//! declarations.

use crate::compiler::model::{DeclarationKind, join_path};
use crate::syntax::ast::{
    Binding, Closure, Declaration, DeclarationForm, Expression, Function, ImplBlock, Parameter,
    TypeDeclaration, TypeDeclarationKind, TypeExpr, TypeField, Use, VariantDeclaration,
};
use crate::syntax::token::TokenKind;

use super::{ParseError, Parser};

impl<'a> Parser<'a> {
    /// The kind the current token opens, when it opens a declaration. The
    /// marking from the surface form to the model's kind lives here, once.
    fn declaration_kind(&self, lookahead: usize) -> Option<DeclarationKind> {
        match &self.peek(lookahead).kind {
            TokenKind::Fn => Some(DeclarationKind::Function),
            TokenKind::Let => Some(DeclarationKind::ModuleValue),
            TokenKind::Rec => Some(DeclarationKind::Rec),
            TokenKind::Enum => Some(DeclarationKind::Enum),
            TokenKind::Impl => Some(DeclarationKind::Impl),
            _ => None,
        }
    }

    /// Parse one declaration: a function, a module value, a nominal type, or an
    /// `impl` block.
    pub(super) fn parse_declaration(&mut self) -> Result<Declaration, ParseError> {
        // `pub` is a production only for a kind that takes visibility. For a
        // `let` or an `impl` it is not consumed, so the declaration dispatch
        // reports it as an ordinary unexpected token.
        let public = self
            .declaration_kind(1)
            .is_some_and(DeclarationKind::takes_visibility)
            && self.eat(&TokenKind::Pub);
        let kind = match self.declaration_kind(0) {
            Some(kind) => kind,
            None => {
                return Err(self.error_here(format!(
                    "expected a declaration, found {}",
                    self.describe_current()
                )));
            }
        };
        let form = match &self.current().kind {
            TokenKind::Fn => DeclarationForm::Function(self.parse_function()?),
            TokenKind::Let => DeclarationForm::Binding(self.parse_let_binding()?),
            TokenKind::Rec => DeclarationForm::Type(self.parse_type_declaration()?),
            TokenKind::Enum => DeclarationForm::Type(self.parse_enum_declaration()?),
            TokenKind::Impl => DeclarationForm::Impl(self.parse_impl_block()?),
            _ => {
                return Err(self.error_here(format!(
                    "expected a declaration, found {}",
                    self.describe_current()
                )));
            }
        };
        let (name, name_span) = match &form {
            DeclarationForm::Function(function) => (function.name.clone(), function.name_span),
            DeclarationForm::Binding(binding) => (binding.name.clone(), binding.name_span),
            DeclarationForm::Type(declaration) => (declaration.name.clone(), declaration.name_span),
            // An `impl` block introduces no name of its own: it files its
            // members under the type it writes.
            DeclarationForm::Impl(block) => (
                block.ty.named_path().map(join_path).unwrap_or_default(),
                block.ty.span(),
            ),
        };
        Ok(Declaration {
            kind,
            public,
            name,
            name_span,
            form,
        })
    }

    /// Parse an `impl Type { fn ...; };` block. Every function in the block is
    /// filed under the block's type; a function's own `pub` lets another
    /// namespace reach it. The target is a written type, resolved later as any
    /// other written type is.
    fn parse_impl_block(&mut self) -> Result<ImplBlock, ParseError> {
        let start = self.advance().start;
        let ty = self.parse_type_expr("in an impl block")?;
        self.expect(&TokenKind::LBrace, "to open the impl block")?;
        let mut functions = Vec::new();
        while !self.check(&TokenKind::RBrace) {
            let public = self.eat(&TokenKind::Pub);
            if !self.check(&TokenKind::Fn) {
                return Err(self.error_here(format!(
                    "expected a function in the impl block, found {}",
                    self.describe_current()
                )));
            }
            let function = self.parse_function()?;
            functions.push(Declaration {
                kind: DeclarationKind::Impl,
                public,
                name: function.name.clone(),
                name_span: function.name_span,
                form: DeclarationForm::Function(function),
            });
        }
        self.expect(&TokenKind::RBrace, "to close the impl block")?;
        let span = self.span_through_semicolon(start, "after the impl block")?;
        Ok(ImplBlock {
            ty,
            functions,
            span,
        })
    }

    /// Parse a `use a::b;` item or a `use a::{b, c};` group. One level of
    /// braces names several things under one prefix.
    pub(super) fn parse_use(&mut self) -> Result<Use, ParseError> {
        let start = self.advance().start;
        // The path may end at `::{`, so the separator before a group is read
        // here rather than by the general path parser.
        let (first, _) = self.expect_identifier("in a use path")?;
        let mut path = vec![first.to_owned()];
        let mut group = false;
        while self.eat(&TokenKind::PathSep) {
            if self.check(&TokenKind::LBrace) {
                group = true;
                break;
            }
            let (segment, _) = self.expect_identifier("in a use path")?;
            path.push(segment.to_owned());
        }
        let mut paths = Vec::new();
        if group {
            let (names, group_span) = self.parse_delimited(
                &TokenKind::LBrace,
                &TokenKind::RBrace,
                "to open the use group",
                "to close the use group",
                |parser| {
                    parser
                        .expect_identifier("in a use group")
                        .map(|(name, _)| name.to_owned())
                },
            )?;
            if names.is_empty() {
                return Err(ParseError {
                    span: group_span,
                    message: "a use group must name at least one thing".to_owned(),
                });
            }
            for name in names {
                let mut full = path.clone();
                full.push(name);
                paths.push(full);
            }
        } else {
            paths.push(path);
        }
        let span = self.span_through_semicolon(start, "after the use path")?;
        Ok(Use { paths, span })
    }

    /// Parse a `rec Name { field: Type, ... };` type declaration.
    fn parse_type_declaration(&mut self) -> Result<TypeDeclaration, ParseError> {
        let start = self.advance().start;
        let (name, name_span) = self.expect_identifier("in a type declaration")?;
        let parameters = self.parse_type_parameters()?;
        let (fields, _) = self.parse_delimited(
            &TokenKind::LBrace,
            &TokenKind::RBrace,
            "to open the type",
            "to close the type",
            Self::parse_type_field,
        )?;
        let span = self.span_through_semicolon(start, "after the type declaration")?;
        Ok(TypeDeclaration {
            name: name.to_owned(),
            name_span,
            parameters,
            kind: TypeDeclarationKind::Rec(fields),
            span,
        })
    }

    /// Parse an `enum Name { Variant, Variant(Type, ...), };` declaration.
    fn parse_enum_declaration(&mut self) -> Result<TypeDeclaration, ParseError> {
        let start = self.advance().start;
        let (name, name_span) = self.expect_identifier("in an enum declaration")?;
        let parameters = self.parse_type_parameters()?;
        let (variants, _) = self.parse_delimited(
            &TokenKind::LBrace,
            &TokenKind::RBrace,
            "to open the enum",
            "to close the enum",
            Self::parse_variant_declaration,
        )?;
        let span = self.span_through_semicolon(start, "after the enum declaration")?;
        Ok(TypeDeclaration {
            name: name.to_owned(),
            name_span,
            parameters,
            kind: TypeDeclarationKind::Enum(variants),
            span,
        })
    }

    /// Parse a `fn(T1, T2)` or `fn(T1, T2) -> R` function type. An absent
    /// arrow means the function returns no value, so `returns` is `None`.
    fn parse_function_type(&mut self) -> Result<TypeExpr, ParseError> {
        let start = self.advance();
        let (parameters, delimiter_span) = self.parse_delimited(
            &TokenKind::LParen,
            &TokenKind::RParen,
            "to open the function type parameters",
            "to close the function type parameters",
            |parser| parser.parse_type_expr("as a function parameter type"),
        )?;
        let returns = if self.eat(&TokenKind::Arrow) {
            Some(Box::new(
                self.parse_type_expr("as the function return type")?,
            ))
        } else {
            None
        };
        let span = match &returns {
            Some(returns) => start.merge(delimiter_span).merge(returns.span()),
            None => start.merge(delimiter_span),
        };
        Ok(TypeExpr::Function {
            parameters,
            returns,
            span,
        })
    }

    /// Parse an optional `<A, B>` type-parameter list.
    fn parse_type_parameters(&mut self) -> Result<Vec<String>, ParseError> {
        if !self.check(&TokenKind::Less) {
            return Ok(Vec::new());
        }
        let (parameters, _) = self.parse_delimited(
            &TokenKind::Less,
            &TokenKind::Greater,
            "to open the type parameters",
            "to close the type parameters",
            |parser| {
                parser
                    .expect_identifier("as a type parameter")
                    .map(|(name, _)| name.to_owned())
            },
        )?;
        Ok(parameters)
    }

    /// Parse one `Name` or `Name(Type, ...)` variant of an enum. `pub` before
    /// the name lets another namespace construct and match it.
    fn parse_variant_declaration(&mut self) -> Result<VariantDeclaration, ParseError> {
        let public = self.eat(&TokenKind::Pub);
        let (name, name_span) = self.expect_identifier("in an enum variant")?;
        let mut end = name_span;
        let payload = if self.check(&TokenKind::LParen) {
            let (payload, payload_span) = self.parse_delimited(
                &TokenKind::LParen,
                &TokenKind::RParen,
                "to open the variant payload",
                "to close the variant payload",
                |parser| parser.parse_type_expr("as a variant payload"),
            )?;
            end = payload_span;
            payload
        } else {
            Vec::new()
        };
        let span = name_span.merge(end);
        Ok(VariantDeclaration {
            name: name.to_owned(),
            name_span,
            public,
            payload,
            span,
        })
    }

    /// Parse one `name: Type` field of a type declaration. `pub` before the
    /// name lets another namespace read and write it.
    fn parse_type_field(&mut self) -> Result<TypeField, ParseError> {
        let public = self.eat(&TokenKind::Pub);
        let (name, name_span) = self.expect_identifier("in a type field")?;
        let ty = self.parse_type_annotation("in a type field")?;
        let span = name_span.merge(ty.span());
        Ok(TypeField {
            name: name.to_owned(),
            name_span,
            public,
            ty,
            span,
        })
    }

    /// Parse a `fn name(parameters) -> type? { ... };` declaration.
    pub(super) fn parse_function(&mut self) -> Result<Function, ParseError> {
        let start = self.advance().start;
        let (name, name_span) = self.expect_identifier("in a function declaration")?;
        let type_parameters = self.parse_type_parameters()?;
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
            name: name.to_owned(),
            name_span,
            type_parameters,
            parameters,
            return_type,
            body,
            span,
        })
    }

    /// Parse an anonymous function literal: `fn(a: T) -> R { ... }`. In
    /// expression position it is a closure value.
    pub(super) fn parse_closure(&mut self) -> Result<Expression, ParseError> {
        let keyword = self.advance();
        let (parameters, _) = self.parse_delimited(
            &TokenKind::LParen,
            &TokenKind::RParen,
            "after `fn`",
            "after the parameter list",
            Self::parse_parameter,
        )?;
        let return_type = self.parse_return_type()?;
        let body = self.parse_block()?;
        let close = self.tokens[self.position.saturating_sub(1)].span;
        Ok(Expression::Closure(Box::new(Closure {
            parameters,
            return_type,
            body,
            span: keyword.merge(close),
        })))
    }

    /// Parse one function parameter: the name, then a `<type>` annotation.
    fn parse_parameter(&mut self) -> Result<Parameter, ParseError> {
        let (name, span) = self.expect_identifier("in the parameter list")?;
        let ty = self.parse_type_annotation("in the parameter list")?;
        Ok(Parameter {
            ty,
            name: name.to_owned(),
            span,
        })
    }

    /// Parse an optional `-> type` return type after the parameter list. An
    /// absent arrow means the function returns no value.
    fn parse_return_type(&mut self) -> Result<Option<TypeExpr>, ParseError> {
        if !self.eat(&TokenKind::Arrow) {
            return Ok(None);
        }
        Ok(Some(self.parse_type_expr("after `->`")?))
    }

    /// Parse one type: a written name applied to any type arguments, or a
    /// `fn(...)` function type. Every name resolves through its declaration, so
    /// a built-in and a user type share one grammar.
    pub(super) fn parse_type_expr(&mut self, context: &str) -> Result<TypeExpr, ParseError> {
        if self.check(&TokenKind::Fn) {
            return self.parse_function_type();
        }
        if let TokenKind::Ident = self.current().kind {
            let (path, path_span) = self.parse_path_segments(context)?;
            let mut span = path_span;
            let arguments = if self.check(&TokenKind::Less) {
                let (arguments, arguments_span) = self.parse_delimited(
                    &TokenKind::Less,
                    &TokenKind::Greater,
                    "to open the type arguments",
                    "to close the type arguments",
                    |parser| parser.parse_type_expr("as a type argument"),
                )?;
                span = span.merge(arguments_span);
                arguments
            } else {
                Vec::new()
            };
            return Ok(TypeExpr::Named {
                path,
                arguments,
                span,
            });
        }
        Err(self.error_here(format!(
            "expected a type {context}, found {}",
            self.describe_current()
        )))
    }

    /// Parse a `: type` annotation, as on a binding, a parameter, or a field.
    fn parse_type_annotation(&mut self, context: &str) -> Result<TypeExpr, ParseError> {
        self.expect(&TokenKind::Colon, context)?;
        self.parse_type_expr(context)
    }

    /// Parse a `let name<type> = expression;` declaration or statement.
    pub(super) fn parse_let_binding(&mut self) -> Result<Binding, ParseError> {
        let start = self.advance().start;
        self.parse_binding_tail(start)
    }

    /// Parse the name, `<type>` annotation, and initializer of a binding. The
    /// `let` is already consumed, and `start` is where the binding began.
    fn parse_binding_tail(&mut self, start: usize) -> Result<Binding, ParseError> {
        let (name, name_span) = self.expect_identifier("in a binding")?;
        let ty = self.parse_type_annotation("in a binding")?;
        self.expect(&TokenKind::Equals, "after the binding type")?;
        let value = self.parse_expression()?;
        let span = self.span_through_semicolon(start, "after the initializer")?;
        Ok(Binding {
            ty,
            name: name.to_owned(),
            name_span,
            value,
            span,
        })
    }
}
