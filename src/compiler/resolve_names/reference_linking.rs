//! Linking references and binding names in a body.
//!
//! A local binding gets a node of its own, a match pattern binds the names its
//! arm body reads, and a `.member` that names an enum variant resolves to that
//! variant rather than to a field.

use crate::compiler::{Diagnostic, duplicate_name};
use crate::model::{DeclarationId, Pattern, TypeId, TypeKind};
use crate::syntax::Expression as ParsedExpression;

use super::builder::{Builder, Skeleton, Slot};
use super::linking::LocalDeclaration;
use super::scopes::{Context, Scopes};

impl Builder {
    /// Create a local binding node owned by a function, rejecting a name that
    /// is already visible in this body. A local may shadow a module name: a
    /// bare name reads the local first, and `::` reaches the module name.
    pub(super) fn declare_local(
        &mut self,
        scopes: &Scopes,
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
        if scopes.find(name).is_some() {
            return Err(Diagnostic::new(reported, name_span, duplicate_name(name)));
        }
        let order = self.skeletons[owner.0].order;
        let id = self.create_node(
            Skeleton {
                name: name.to_owned(),
                name_span,
                public: false,
                first_parameter_head: None,
                file: None,
                namespace,
                order,
                syntax_index: None,
            },
            Some(owner),
            Some(Slot::Binding(ty)),
        );
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
    /// that no value shadows: `Option.some`, `std::Option.none`.
    pub(super) fn variant_member(
        &self,
        context: &Context,
        receiver: &ParsedExpression,
        member: &str,
    ) -> Result<Option<(TypeId, usize)>, Diagnostic> {
        let ParsedExpression::Name { root, path, span } = receiver else {
            return Ok(None);
        };
        if self.resolve(context, path, *root, *span).is_ok() {
            return Ok(None);
        }
        let mut full = path.clone();
        full.push(member.to_owned());
        if let Some(found) = self.resolve_variant(context.namespace, &full, *root) {
            return Ok(Some(found));
        }
        // The receiver names a type, but not this variant: say so precisely.
        if self.find_type(context.namespace, path, *root).is_some() {
            let (member, type_path) = full
                .split_last()
                .expect("the variant path has at least two segments");
            return Err(Diagnostic::new(
                &self.files[context.file.0].path,
                *span,
                format!(
                    "unknown enum variant `{}.{member}`",
                    crate::model::join_path(type_path, *root)
                ),
            ));
        }
        Ok(None)
    }

    /// Link a `Name.variant(bind, ...)` pattern into a variant pattern, creating
    /// a local for each binding so the arm body can read the payload. `None`
    /// when the pattern does not name a variant.
    fn link_variant_pattern(
        &mut self,
        pattern: &ParsedExpression,
        context: &mut Context,
    ) -> Result<Option<Pattern>, Diagnostic> {
        let (ty, index, arguments, span) = match pattern {
            ParsedExpression::MethodCall {
                receiver,
                method,
                arguments,
                span,
                ..
            } => {
                let Some((ty, index)) = self.variant_member(context, receiver, method)? else {
                    return Ok(None);
                };
                (ty, index, arguments.as_slice(), *span)
            }
            ParsedExpression::Field {
                target, name, span, ..
            } => {
                let Some((ty, index)) = self.variant_member(context, target, name)? else {
                    return Ok(None);
                };
                (ty, index, &[] as &[ParsedExpression], *span)
            }
            _ => return Ok(None),
        };
        let reported = self.files[context.file.0].path.clone();
        let (variant_name, declared) = {
            let TypeKind::Enum(variants) = &self.types[ty.0].kind else {
                unreachable!("resolve_variant only finds an enum");
            };
            (
                variants[index].name.clone(),
                variants[index].payload.clone(),
            )
        };
        if arguments.len() != declared.len() {
            return Err(Diagnostic::new(
                reported,
                span,
                format!(
                    "`{variant_name}` takes {} payload binding(s), found {}",
                    declared.len(),
                    arguments.len()
                ),
            ));
        }
        let mut linked = Vec::with_capacity(arguments.len());
        for (argument, field_type) in arguments.iter().zip(&declared) {
            let ParsedExpression::Name {
                root: false,
                path,
                span,
            } = argument
            else {
                return Err(Diagnostic::new(
                    &self.files[context.file.0].path,
                    argument.span(),
                    "a variant pattern binds a name or `_`",
                ));
            };
            let [name] = path.as_slice() else {
                return Err(Diagnostic::new(
                    &self.files[context.file.0].path,
                    *span,
                    "a variant pattern binds a name or `_`",
                ));
            };
            if name == "_" {
                linked.push(None);
                continue;
            }
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
            context.scopes.declare(name, declaration);
            linked.push(Some(declaration));
        }
        Ok(Some(Pattern::Variant {
            ty,
            index,
            bindings: linked,
            span,
        }))
    }
}
