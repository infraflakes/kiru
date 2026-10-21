//! Linking references and binding names in a body.
//!
//! A local binding gets a node of its own, a match pattern binds the names its
//! arm body reads, and a `.member` that names an enum variant resolves to that
//! variant rather than to a field.

use crate::compiler::model::{DeclarationId, DeclarationKind, NamespaceId, Pattern, TypeHead};
use crate::compiler::{Diagnostic, duplicate_name};
use crate::syntax::Expression as ParsedExpression;

use super::body_linking::LocalDeclaration;
use super::builder::{Builder, Skeleton, Slot, Variant};
use super::scopes::{Context, Scopes};

impl Builder {
    /// Create a local binding node owned by a function, rejecting a name that
    /// is already visible in this body. A local may shadow a module name: a
    /// bare name reads the local first, and a qualified path reaches the
    /// module name.
    pub(super) fn declare_local(
        &mut self,
        scopes: &mut Scopes,
        local: LocalDeclaration<'_>,
    ) -> Result<DeclarationId, Diagnostic> {
        let LocalDeclaration {
            file,
            namespace,
            name,
            name_span,
            owner,
            ty,
        } = local;
        let reported = &self.files[file.0].path;
        let symbol = self.intern.intern(name);
        if let Some(existing) = scopes.find(symbol) {
            return Err(
                Diagnostic::new(reported, name_span, duplicate_name(name)).with_related(
                    reported,
                    self.declarations[existing.0].name_span,
                    crate::compiler::messages::first_declaration(name),
                ),
            );
        }
        let id = self.create_node(
            Skeleton {
                kind: DeclarationKind::Local,
                name: name.to_owned(),
                name_span,
                public: false,
                receiver_head: None,
                associated_head: None,
                file: None,
                namespace,
            },
            Some(owner),
            Some(Slot::Binding(ty)),
        );
        scopes.declare(symbol, id);
        Ok(id)
    }
    /// Link a match pattern. A pattern that names an enum variant becomes a
    /// variant pattern; anything else is an equality atom.
    pub(super) fn link_pattern(
        &mut self,
        pattern: &ParsedExpression,
        context: &mut Context,
    ) -> Result<Pattern, Diagnostic> {
        if let Some(linked) = self.link_variant_pattern(pattern, context)? {
            return Ok(linked);
        }
        Ok(Pattern::Value(self.link_expression(pattern, context)?))
    }

    /// The enum variant a `.member` names, when the receiver is a bare type path
    /// that no value shadows: `Option.some`, `std::Option.none`. A type that is
    /// not an enum has no variants, so `None` lets the associated-function
    /// lookup run.
    pub(super) fn variant_member(
        &self,
        context: &Context,
        receiver: &ParsedExpression,
        member: &str,
    ) -> Result<Option<Variant<'_>>, Diagnostic> {
        let ParsedExpression::Name { path, span } = receiver else {
            return Ok(None);
        };
        if self.resolve(context, path, *span).is_ok() {
            return Ok(None);
        }
        let mut full = path.clone();
        full.push(member.to_owned());
        if let Some(found) = self.resolve_variant(context.namespace, &full) {
            return Ok(Some(found));
        }
        // The receiver names an enum, but not this variant: say so precisely.
        if let Some(id) = self.find_type(context.namespace, path)
            && self.types[id.0].group().has_variants()
        {
            let (member, type_path) = full
                .split_last()
                .expect("the variant path has at least two segments");
            return Err(Diagnostic::new(
                &self.files[context.file.0].path,
                *span,
                format!(
                    "unknown enum variant `{}.{member}`",
                    crate::compiler::model::join_path(type_path)
                ),
            ));
        }
        Ok(None)
    }

    /// The associated function a `.member` on a type name names: a function in
    /// an `impl` block for the type whose first parameter is not the type.
    /// `None` when the receiver names a value or no type. A type receiver that
    /// names no such function is an error, because a type is not a value.
    pub(super) fn associated_member(
        &self,
        context: &Context,
        receiver: &ParsedExpression,
        member: &str,
    ) -> Result<Option<DeclarationId>, Diagnostic> {
        let ParsedExpression::Name { path, span } = receiver else {
            return Ok(None);
        };
        // A receiver that names a value is an ordinary method call.
        if self.resolve(context, path, *span).is_ok() {
            return Ok(None);
        }
        let Some(id) = self.find_type(context.namespace, path) else {
            return Ok(None);
        };
        let head = TypeHead::Named(id);
        let found = self.visible_associates(context, member, head);
        match found.as_slice() {
            [] => Err(Diagnostic::new(
                &self.files[context.file.0].path,
                *span,
                format!(
                    "`{}` has no associated function `{member}`",
                    crate::compiler::model::join_path(path)
                ),
            )),
            [declaration] => Ok(Some(*declaration)),
            _ => Err(Diagnostic::new(
                &self.files[context.file.0].path,
                *span,
                format!("`{member}` is ambiguous for this type"),
            )),
        }
    }

    /// Every associated function for `head` named `member` that `context` may
    /// reach. A bound function is gated by `pub` alone, never by a `use`, so
    /// the whole program is searched.
    fn visible_associates(
        &self,
        context: &Context,
        member: &str,
        head: TypeHead,
    ) -> Vec<DeclarationId> {
        let Some(symbol) = self.intern.resolve(member) else {
            return Vec::new();
        };
        let mut found = Vec::new();
        for (index, namespace) in self.namespaces.iter().enumerate() {
            if let Some(declaration) = namespace.associates.get(head, symbol)
                && self.bound_reachable(context.namespace, NamespaceId(index), declaration)
            {
                found.push(declaration);
            }
        }
        found
    }

    /// The bound function a `.member` on a type name names, as a value: a
    /// method or an associated function filed under that type. Methods come
    /// bundled with their type, so `Type.name` is the function value and
    /// `Type.name(...)` is the call. `None` when the receiver names a value or
    /// no type, or when the type carries no such function.
    pub(super) fn bound_member(
        &self,
        context: &Context,
        receiver: &ParsedExpression,
        member: &str,
    ) -> Result<Option<DeclarationId>, Diagnostic> {
        let ParsedExpression::Name { path, span } = receiver else {
            return Ok(None);
        };
        // A receiver that names a value is an ordinary field access.
        if self.resolve(context, path, *span).is_ok() {
            return Ok(None);
        }
        let Some(id) = self.find_type(context.namespace, path) else {
            return Ok(None);
        };
        let head = TypeHead::Named(id);
        let mut found = self.visible_associates(context, member, head);
        found.extend(self.visible_methods(context, member, head));
        match found.as_slice() {
            [] => Ok(None),
            [declaration] => Ok(Some(*declaration)),
            _ => Err(Diagnostic::new(
                &self.files[context.file.0].path,
                *span,
                format!("`{member}` is ambiguous for this type"),
            )),
        }
    }

    /// Link a `Name.variant(bind, ...)` pattern into a variant pattern, creating
    /// a local for each binding so the arm body can read the payload. `None`
    /// when the pattern does not name a variant.
    fn link_variant_pattern(
        &mut self,
        pattern: &ParsedExpression,
        context: &mut Context,
    ) -> Result<Option<Pattern>, Diagnostic> {
        let (found, arguments, span) = match pattern {
            ParsedExpression::MethodCall {
                receiver,
                method,
                arguments,
                span,
                ..
            } => {
                let Some(found) = self.variant_member(context, receiver, method)? else {
                    return Ok(None);
                };
                (found, arguments.as_slice(), *span)
            }
            ParsedExpression::Field {
                target, name, span, ..
            } => {
                let Some(found) = self.variant_member(context, target, name)? else {
                    return Ok(None);
                };
                (found, &[] as &[ParsedExpression], *span)
            }
            _ => return Ok(None),
        };
        let reported = self.files[context.file.0].path.clone();
        let variant_name = found.declaration.name.clone();
        let declared = found.declaration.payload.clone();
        let ty = found.ty;
        let index = found.index;
        if let Some(message) = crate::compiler::messages::variant_payload_count(
            &variant_name,
            declared.len(),
            arguments.len(),
            "payload binding",
        ) {
            return Err(Diagnostic::new(reported, span, message));
        }
        let mut linked = Vec::with_capacity(arguments.len());
        for (argument, field_type) in arguments.iter().zip(&declared) {
            match argument {
                // The wildcard payload binds nothing: the arm matches the
                // variant without naming the position's value.
                ParsedExpression::Wildcard { .. } => linked.push(None),
                ParsedExpression::Name { path, span } => {
                    let [name] = path.as_slice() else {
                        return Err(Diagnostic::new(
                            &self.files[context.file.0].path,
                            *span,
                            "a variant pattern binds a name or `_`",
                        ));
                    };
                    let declaration = self.declare_local(
                        context.scopes,
                        LocalDeclaration {
                            file: context.file,
                            namespace: context.namespace,
                            name,
                            name_span: *span,
                            owner: context.current,
                            ty: field_type.clone(),
                        },
                    )?;
                    linked.push(Some(declaration));
                }
                _ => {
                    return Err(Diagnostic::new(
                        &self.files[context.file.0].path,
                        argument.span(),
                        "a variant pattern binds a name or `_`",
                    ));
                }
            }
        }
        Ok(Some(Pattern::Variant {
            ty,
            index,
            bindings: linked,
            span,
        }))
    }
}
