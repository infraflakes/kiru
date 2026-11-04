//! Linking references and binding names in a body.
//!
//! A local binding gets a node of its own, a match pattern binds the names its
//! arm body reads, and a `.member` that names an enum variant resolves to that
//! variant rather than to a field.

use crate::compiler::graph::{DeclarationId, DeclarationKind, NamespaceId, Pattern, TypeHead};
use crate::compiler::{Diagnostic, duplicate_name};
use crate::syntax::{Expression as ParsedExpression, Span};

use super::body_linking::LocalDeclaration;
use super::builder::{Builder, Skeleton, Slot};
use super::scopes::{Context, Scopes};

/// A member a `::` path reaches on a type: a variant construction or a
/// associated function.
pub(super) enum QualifiedMember {
    Variant { ty: DeclarationId, index: usize },
    Function(DeclarationId),
}

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
                    crate::compiler::diagnostic::first_declaration(name),
                ),
            );
        }
        let id = self.create_node(
            Skeleton {
                kind: DeclarationKind::Local,
                binding: None,
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

    /// The type a receiver names, when the receiver is a type and not a value.
    /// A `::` path is the only way to a type's members, so `.` on a type
    /// reports this path instead of resolving a field.
    pub(super) fn type_path_of<'a>(
        &self,
        context: &Context,
        receiver: &'a ParsedExpression,
    ) -> Option<&'a [String]> {
        let ParsedExpression::Name { path, span } = receiver else {
            return None;
        };
        if self.resolve(context, path, *span).is_ok() {
            return None;
        }
        self.find_type(context.namespace, path)
            .map(|_| path.as_slice())
    }

    /// The member a `::` path reaches on a type: the type's variant when the
    /// member names one, otherwise the associated function in the type's
    /// `impl` block. A path whose prefix names no type is not qualified and
    /// returns `None`, so the ordinary name pools answer it.
    pub(super) fn qualified_member(
        &self,
        context: &Context,
        path: &[String],
        span: Span,
    ) -> Result<Option<QualifiedMember>, Diagnostic> {
        let (member, type_path) = path
            .split_last()
            .expect("a name path has at least one segment");
        if type_path.is_empty() {
            return Ok(None);
        }
        let Some(id) = self.find_type(context.namespace, type_path) else {
            return Ok(None);
        };
        if let Some(found) = self.resolve_variant(context.namespace, path) {
            return Ok(Some(QualifiedMember::Variant {
                ty: found.ty,
                index: found.index,
            }));
        }
        let head = TypeHead::Named(id);
        let mut found = self.visible_associates(context, member, head);
        found.extend(self.visible_methods(context, member, head));
        match found.as_slice() {
            [] => {
                let body = self.declarations[id.0]
                    .type_body()
                    .expect("a named type names a type declaration");
                if body.group().has_variants() {
                    return Err(Diagnostic::new(
                        &self.files[context.file.0].path,
                        span,
                        format!(
                            "unknown enum variant `{}::{member}`",
                            crate::compiler::graph::join_path(type_path)
                        ),
                    ));
                }
                Err(Diagnostic::new(
                    &self.files[context.file.0].path,
                    span,
                    format!(
                        "`{}` has no function `{member}`",
                        crate::compiler::graph::join_path(type_path)
                    ),
                ))
            }
            [declaration] => Ok(Some(QualifiedMember::Function(*declaration))),
            _ => Err(Diagnostic::new(
                &self.files[context.file.0].path,
                span,
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

    /// Link a `Name::variant(bind, ...)` pattern into a variant pattern,
    /// creating a local for each binding so the arm body can read the payload.
    /// `None` when the pattern does not name a variant.
    fn link_variant_pattern(
        &mut self,
        pattern: &ParsedExpression,
        context: &mut Context,
    ) -> Result<Option<Pattern>, Diagnostic> {
        let (found, arguments, span) = match pattern {
            ParsedExpression::Call {
                callee,
                arguments,
                span,
                ..
            } => {
                let Some(found) = self.resolve_variant(context.namespace, callee) else {
                    return Ok(None);
                };
                (found, arguments.as_slice(), *span)
            }
            ParsedExpression::Name { path, span } => {
                let Some(found) = self.resolve_variant(context.namespace, path) else {
                    return Ok(None);
                };
                (found, &[] as &[ParsedExpression], *span)
            }
            ParsedExpression::MethodCall {
                receiver,
                method,
                span,
                ..
            } => {
                if let Some(type_path) = self.type_path_of(context, receiver) {
                    return Err(Diagnostic::new(
                        &self.files[context.file.0].path,
                        *span,
                        format!(
                            "a type is not a value; write `{}::{method}`",
                            crate::compiler::graph::join_path(type_path)
                        ),
                    ));
                }
                return Ok(None);
            }
            _ => return Ok(None),
        };
        let reported = self.files[context.file.0].path.clone();
        let variant_name = found.declaration.name.clone();
        let declared = found.declaration.payload.clone();
        let ty = found.ty;
        let index = found.index;
        if let Some(message) = crate::compiler::diagnostic::variant_payload_count(
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
