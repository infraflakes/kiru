//! Declaration level grammar: modules, imports, functions, variables, recs,
//! and nominal type declarations.

use crate::syntax::ast::{
    Binding, Closure, Declaration, Expression, Function, Import, Module, Parameter,
    TypeDeclaration, TypeDeclarationKind, TypeExpr, TypeField, Use, VariantDeclaration,
};
use crate::syntax::token::TokenKind;

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
        let mut uses = Vec::new();
        while !self.check(&TokenKind::RBrace) {
            if self.check(&TokenKind::Eof) {
                return Err(
                    self.error_here("expected `}` to close the module, found the end of the file")
                );
            }
            if self.check(&TokenKind::Use) {
                uses.push(self.parse_use()?);
                continue;
            }
            declarations.push(self.parse_declaration()?);
        }
        self.expect(&TokenKind::RBrace, "to close the module")?;
        self.leave_nesting();
        let span = self.span_through_semicolon(start, "after the module")?;
        Ok(Module {
            path,
            declarations,
            uses,
            span,
        })
    }

    /// Parse one declaration: a function, a module value, or a nominal type.
    pub(super) fn parse_declaration(&mut self) -> Result<Declaration, ParseError> {
        // `pub` marks a declaration another namespace may reach.
        let public = self.eat(&TokenKind::Pub);
        let mut declaration = match &self.current().kind {
            TokenKind::Fn => Declaration::Function(self.parse_function()?),
            // At the top level, `rec` opens a nominal type declaration,
            // `enum` opens an enum type declaration, and a bodyless `type`
            // declares a built-in type the runtime provides.
            TokenKind::Rec => Declaration::Type(self.parse_type_declaration()?),
            TokenKind::Enum => Declaration::Type(self.parse_enum_declaration()?),
            TokenKind::Type => Declaration::Type(self.parse_builtin_type_declaration()?),
            TokenKind::Let => Declaration::Binding(self.parse_let_binding()?),
            other => {
                return Err(self.error_here(format!(
                    "expected a declaration, found {}",
                    other.describe()
                )));
            }
        };
        match &mut declaration {
            Declaration::Function(function) => function.public = public,
            Declaration::Binding(binding) => binding.public = public,
            Declaration::Type(type_declaration) => type_declaration.public = public,
        }
        Ok(declaration)
    }

    /// Parse a `use a::b;` item: the namespace path whose public names this
    /// namespace gains.
    pub(super) fn parse_use(&mut self) -> Result<Use, ParseError> {
        let start = self.advance().start;
        let (path, _) = self.parse_path_segments("in a use path")?;
        let span = self.span_through_semicolon(start, "after the use path")?;
        Ok(Use { path, span })
    }

    /// Parse an `import "path";` declaration.
    pub(super) fn parse_import(&mut self) -> Result<Import, ParseError> {
        let start = self.advance().start;
        let (path, _) = self.expect_text("after `import`")?;
        let span = self.span_through_semicolon(start, "after the import path")?;
        Ok(Import { path, span })
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
            name,
            name_span,
            public: false,
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
            name,
            name_span,
            public: false,
            parameters,
            kind: TypeDeclarationKind::Enum(variants),
            span,
        })
    }

    /// Parse a bodyless `type Name<parameters>;` declaration: a built-in type
    /// the runtime provides. The marker is the missing body.
    fn parse_builtin_type_declaration(&mut self) -> Result<TypeDeclaration, ParseError> {
        let start = self.advance().start;
        let (name, name_span) = self.expect_identifier("in a type declaration")?;
        let parameters = self.parse_type_parameters()?;
        let span = self.span_through_semicolon(start, "after the type declaration")?;
        Ok(TypeDeclaration {
            name,
            name_span,
            public: false,
            parameters,
            kind: TypeDeclarationKind::Builtin,
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
                    .map(|(name, _)| name)
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
            name,
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
            name,
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
            name,
            name_span,
            public: false,
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
        Ok(Parameter { ty, name, span })
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
        if matches!(
            self.current().kind,
            TokenKind::Ident(_) | TokenKind::PathSep
        ) {
            let (root, path, path_span) = self.parse_rooted_path(context)?;
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
                root,
                path,
                arguments,
                span,
            });
        }
        Err(self.error_here(format!(
            "expected a type {context}, found {}",
            self.current().kind.describe()
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
            name,
            name_span,
            public: false,
            value,
            span,
        })
    }
}
