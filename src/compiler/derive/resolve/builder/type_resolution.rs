//! Resolving written types to the type system.

use crate::compiler::Diagnostic;
use crate::compiler::graph::{
    Arity, DeclarationId, FileId, Group, NamespaceId, Type, TypeHead, TypeKind, TypeUse, UseTarget,
    VariantDecl, join_path, namespace_path, reachable, scalar_from_name,
};
use crate::syntax::{Span, TypeExpr};

use super::registration::Builder;

/// One enum variant a written path names: the enum's type, the variant's index,
/// and the declared variant itself. The lookup returns the declaration it
/// resolved, so a caller that needs the variant's name or payload reads it from
/// here rather than looking it up again.
pub(in crate::compiler::derive::resolve) struct Variant<'a> {
    pub(in crate::compiler::derive::resolve) ty: DeclarationId,
    pub(in crate::compiler::derive::resolve) index: usize,
    pub(in crate::compiler::derive::resolve) declaration: &'a VariantDecl,
}

/// The number of type arguments a written name must take: a fixed count for a
/// kind that takes none, or the declaration's parameter count for a kind whose
/// arguments come from its declaration. The group is the one place that
/// difference is declared.
fn type_argument_count(group: Group, declaration_parameters: usize) -> usize {
    match group.arity() {
        Arity::Fixed(count) => count,
        Arity::Declared => declaration_parameters,
    }
}

impl Builder {
    /// Resolve a written type to a `Type`.
    ///
    /// Every written type resolves the same way: the name finds the
    /// declaration it names, and the declaration decides the result — a
    /// built-in marker yields the runtime representation the marker carries,
    /// and a program `rec`/`enum` yields `Type::Named`. A written name
    /// (`txt`, `Vec<T>`, a bare name) differs only in how it
    /// spells the name and arguments.
    pub(in crate::compiler::derive::resolve) fn resolve_type_expr(
        &self,
        expr: &TypeExpr,
        namespace: NamespaceId,
        file: FileId,
        parameters: &[String],
        reported_path: &std::path::Path,
    ) -> Result<Type, Diagnostic> {
        match expr {
            TypeExpr::Function {
                parameters: declared,
                returns,
                ..
            } => {
                let mut resolved = Vec::with_capacity(declared.len());
                for parameter in declared {
                    resolved.push(self.resolve_type_expr(
                        parameter,
                        namespace,
                        file,
                        parameters,
                        reported_path,
                    )?);
                }
                // No arrow means the function returns no value: its function
                // type has no return type, and there is no type that names it.
                let returns = match returns {
                    Some(returns) => Some(Box::new(self.resolve_type_expr(
                        returns,
                        namespace,
                        file,
                        parameters,
                        reported_path,
                    )?)),
                    None => None,
                };
                Ok(Type::Function {
                    parameters: resolved,
                    returns,
                })
            }
            TypeExpr::Named {
                path,
                path_span,
                arguments,
                span,
            } => {
                if path.len() == 1
                    && arguments.is_empty()
                    && parameters.iter().any(|parameter| parameter == &path[0])
                {
                    return Ok(Type::Parameter(path[0].clone()));
                }
                // The written name is a fact of the source: the path it
                // spells and the name the path ends at.
                if let Some(last) = path.last() {
                    self.type_uses.borrow_mut().push(TypeUse {
                        file,
                        path: *path_span,
                        name: Span::new(path_span.end - last.len(), path_span.end),
                    });
                }
                self.resolve_named_type(
                    WrittenType { path, span },
                    arguments,
                    namespace,
                    file,
                    parameters,
                    reported_path,
                )
            }
        }
    }

    /// Resolve a written type by the declaration it names: find the
    /// declaration, check its type-argument count, resolve the arguments, and
    /// read its kind for the result.
    fn resolve_named_type(
        &self,
        written: WrittenType<'_>,
        arguments: &[TypeExpr],
        namespace: NamespaceId,
        file: FileId,
        parameters: &[String],
        reported_path: &std::path::Path,
    ) -> Result<Type, Diagnostic> {
        let WrittenType { path, span } = written;
        let named = self.named_type(written, namespace, reported_path)?;
        // A scalar takes no arguments, so its count is settled before any
        // argument is resolved.
        if let NamedType::Scalar(scalar) = &named {
            let parameter_count = type_argument_count(scalar.group(&self.declarations), 0);
            if parameter_count != arguments.len() {
                return Err(Diagnostic::new(
                    reported_path,
                    *span,
                    format!(
                        "`{}` takes {}, found {}",
                        path[0],
                        crate::compiler::diagnostic::count(parameter_count, "type argument"),
                        arguments.len()
                    ),
                ));
            }
            return Ok(scalar.clone());
        }
        let mut resolved = Vec::with_capacity(arguments.len());
        for argument in arguments {
            resolved.push(self.resolve_type_expr(
                argument,
                namespace,
                file,
                parameters,
                reported_path,
            )?);
        }
        match named {
            NamedType::Scalar(_) => unreachable!("the scalar case was settled above"),
            NamedType::Declared(id) => {
                let declaration = &self.declarations[id.0];
                let body = declaration
                    .type_body()
                    .expect("a named type names a type declaration");
                let parameter_count = type_argument_count(body.group(), body.parameters.len());
                if parameter_count != arguments.len() {
                    return Err(Diagnostic::new(
                        reported_path,
                        *span,
                        format!(
                            "`{}` takes {}, found {}",
                            declaration.name,
                            crate::compiler::diagnostic::count(parameter_count, "type argument"),
                            arguments.len()
                        ),
                    ));
                }
                Ok(Type::Named(id, resolved))
            }
        }
    }

    /// Resolve a written named type path to the declaration it names: a scalar
    /// intrinsic, or the declared type the path reaches. This is the one name
    /// lookup a written type annotation and an `impl` target share. Applying
    /// type arguments and checking their count belongs to the annotation, not
    /// to the lookup.
    fn named_type(
        &self,
        written: WrittenType<'_>,
        namespace: NamespaceId,
        reported_path: &std::path::Path,
    ) -> Result<NamedType, Diagnostic> {
        let WrittenType { path, span } = written;
        // The scalar intrinsics are the language's own types, not
        // declarations: a bare single-segment name names one directly, before
        // any declaration is looked up. Nothing can delete `txt` by deleting a
        // declaration, because there is none. `Vec` is not here: it is an
        // ordinary declaration the compiler registers in `std::dynamic`.
        if path.len() == 1
            && let Some(scalar) = scalar_from_name(&path[0])
        {
            return Ok(NamedType::Scalar(scalar));
        }
        match self.find_type(namespace, path) {
            Some(id) => Ok(NamedType::Declared(id)),
            None => Err(Diagnostic::new(
                reported_path,
                *span,
                format!("unknown type `{}`", join_path(path)),
            )),
        }
    }

    /// Resolve an `impl` block's target to the head every function in the block
    /// is filed under. The block names a type declaration by any path, full,
    /// shortened, or bare; its functions are keyed by the type's head, so a
    /// generic declaration is named without its parameters and a written
    /// argument list changes nothing beneath. Only a group that binds methods
    /// may carry the block.
    pub(in crate::compiler::derive::resolve) fn resolve_impl_target(
        &self,
        ty: &TypeExpr,
        namespace: NamespaceId,
        file: FileId,
        reported_path: &std::path::Path,
    ) -> Result<TypeHead, Diagnostic> {
        let TypeExpr::Named {
            path,
            path_span,
            span,
            ..
        } = ty
        else {
            let resolved = self.resolve_type_expr(ty, namespace, file, &[], reported_path)?;
            return Err(Diagnostic::new(
                reported_path,
                ty.span(),
                format!(
                    "`{}` cannot carry methods",
                    crate::compiler::graph::type_name(&resolved, &self.declarations)
                ),
            ));
        };
        if let Some(last) = path.last() {
            self.type_uses.borrow_mut().push(TypeUse {
                file,
                path: *path_span,
                name: Span::new(path_span.end - last.len(), path_span.end),
            });
        }
        let resolved = match self.named_type(
            WrittenType { path, span },
            namespace,
            reported_path,
        )? {
            NamedType::Scalar(scalar) => scalar,
            NamedType::Declared(id) => {
                let declaration = &self.declarations[id.0];
                if declaration.namespace != namespace {
                    let declared_in = {
                        let declared_path = namespace_path(&self.namespaces, declaration.namespace);
                        if declared_path.is_empty() {
                            "the root namespace".to_owned()
                        } else {
                            format!("`{}`", join_path(&declared_path))
                        }
                    };
                    return Err(Diagnostic::new(
                        reported_path,
                        ty.span(),
                        format!(
                            "`{}` is declared in {declared_in}; its methods must be declared there",
                            declaration.name
                        ),
                    ));
                }
                Type::Named(id, Vec::new())
            }
        };
        if !resolved.group(&self.declarations).binds_methods() {
            return Err(Diagnostic::new(
                reported_path,
                ty.span(),
                format!(
                    "`{}` cannot carry methods",
                    crate::compiler::graph::type_name(&resolved, &self.declarations)
                ),
            ));
        }
        Ok(resolved.head(&self.declarations))
    }

    /// Apply a declared type named by a path to already-resolved arguments. A
    /// written annotation and a native row both reach a declaration through
    /// this one lookup, so the name resolution and the argument count are
    /// enforced the same way for both.
    pub(in crate::compiler::derive::resolve) fn apply_declared_type(
        &self,
        path: &[String],
        span: &Span,
        arguments: Vec<Type>,
        namespace: NamespaceId,
        reported_path: &std::path::Path,
    ) -> Result<Type, Diagnostic> {
        let Some(id) = self.find_type(namespace, path) else {
            return Err(Diagnostic::new(
                reported_path,
                *span,
                format!("unknown type `{}`", join_path(path)),
            ));
        };
        let declared = &self.declarations[id.0];
        let body = declared
            .type_body()
            .expect("a named type names a type declaration");
        let name = declared.name.clone();
        let parameter_count = type_argument_count(body.group(), body.parameters.len());
        if parameter_count != arguments.len() {
            return Err(Diagnostic::new(
                reported_path,
                *span,
                format!(
                    "`{name}` takes {}, found {}",
                    crate::compiler::diagnostic::count(parameter_count, "type argument"),
                    arguments.len()
                ),
            ));
        }
        Ok(Type::Named(id, arguments))
    }

    /// Resolve a `Type.Variant` path to the enum's type and the variant's
    /// index, when the path names a declared enum variant. A path with one
    /// segment is never a variant.
    pub(in crate::compiler::derive::resolve) fn resolve_variant<'a>(
        &'a self,
        namespace: NamespaceId,
        path: &[String],
    ) -> Option<Variant<'a>> {
        let (variant, type_segments) = path.split_last()?;
        if type_segments.is_empty() {
            return None;
        }
        let id = self.find_type(namespace, type_segments)?;
        let body = self.declarations[id.0]
            .type_body()
            .expect("a named type names a type declaration");
        if !body.group().has_variants() {
            return None;
        }
        let TypeKind::Enum(variants) = &body.kind else {
            unreachable!("a group with variants is an enum");
        };
        let index = variants
            .iter()
            .position(|declared| &declared.name == variant)?;
        Some(Variant {
            ty: id,
            index,
            declaration: &variants[index],
        })
    }

    /// The declared type a bare name sees: the current namespace's type pool,
    /// then a type a name-`use` brings in. Type names and value names are
    /// different pools, so a value named `Foo` never hides a type named `Foo`.
    /// The `use`'s target was resolved once, when the namespace's index was
    /// built, so this reads that recording.
    fn bare_type(&self, namespace: NamespaceId, name: &str) -> Option<DeclarationId> {
        let symbol = self.intern.resolve(name)?;
        let namespace_table = &self.namespaces[namespace.0];
        if let Some(found) = namespace_table.types.get(symbol) {
            return Some(found);
        }
        let entries = namespace_table.brought.get(&symbol)?;
        let taken = entries.iter().position(|entry| match entry.target {
            UseTarget::Type(found) => {
                let declaration = &self.declarations[found.0];
                reachable(declaration.public, declaration.namespace, namespace)
            }
            _ => false,
        })?;
        self.record_delivery(entries, taken);
        match entries[taken].target {
            UseTarget::Type(found) => Some(found),
            _ => unreachable!("the position names a type entry"),
        }
    }

    /// Find a declared type by a path. A bare single segment reads the current
    /// namespace and the names it `use`s; a qualified path starts at a tree
    /// name and walks from the root, or at a namespace-`use`'s qualifier and
    /// walks from there. A type reached across a namespace must be `pub`.
    pub(in crate::compiler::derive::resolve) fn find_type(
        &self,
        namespace: NamespaceId,
        path: &[String],
    ) -> Option<DeclarationId> {
        let (name, prefix) = path
            .split_last()
            .expect("a type path has at least one segment");
        if prefix.is_empty() {
            return self.bare_type(namespace, name);
        }
        let target = self.namespace_of_path(namespace, prefix)?;
        let found = self.namespaces[target.0].type_id(&self.intern, name)?;
        let declaration = &self.declarations[found.0];
        reachable(declaration.public, target, namespace).then_some(found)
    }
}

/// A written type name: its path and its span. Every written name resolves
/// through one helper, which takes this.
struct WrittenType<'a> {
    path: &'a [String],
    span: &'a Span,
}

/// The declaration a written named type path names, before type arguments are
/// applied: a scalar intrinsic, or a declared type by its id.
enum NamedType {
    Scalar(Type),
    Declared(DeclarationId),
}
