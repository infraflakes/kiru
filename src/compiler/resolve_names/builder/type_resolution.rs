//! Resolving written types to the type system.

use crate::compiler::Diagnostic;
use crate::compiler::model::{
    Arity, Group, NamespaceId, Type, TypeId, TypeKind, join_path, reachable, scalar_from_name,
};
use crate::syntax::{Span, TypeExpr};

use super::registration::Builder;

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
    pub(in crate::compiler::resolve_names) fn resolve_type_expr(
        &self,
        expr: &TypeExpr,
        namespace: NamespaceId,
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
                arguments,
                span,
            } => {
                if path.len() == 1
                    && arguments.is_empty()
                    && parameters.iter().any(|parameter| parameter == &path[0])
                {
                    return Ok(Type::Parameter(path[0].clone()));
                }
                self.resolve_named_type(
                    WrittenType { path, span },
                    arguments,
                    namespace,
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
        parameters: &[String],
        reported_path: &std::path::Path,
    ) -> Result<Type, Diagnostic> {
        let WrittenType { path, span } = written;
        // The scalar intrinsics are the language's own types, not
        // declarations: a bare single-segment name names one directly, before
        // any declaration is looked up. Nothing can delete `txt` by deleting a
        // declaration, because there is none. `Vec` is not here: it is an
        // ordinary declaration the compiler registers in `std::dynamic`.
        if path.len() == 1
            && let Some(scalar) = scalar_from_name(&path[0])
        {
            let parameter_count = type_argument_count(scalar.group(&self.types), 0);
            if parameter_count != arguments.len() {
                return Err(Diagnostic::new(
                    reported_path,
                    *span,
                    format!(
                        "`{}` takes {}, found {}",
                        path[0],
                        crate::compiler::messages::count(parameter_count, "type argument"),
                        arguments.len()
                    ),
                ));
            }
            return Ok(scalar);
        }
        let Some(id) = self.find_type(namespace, path) else {
            return Err(Diagnostic::new(
                reported_path,
                *span,
                format!("unknown type `{}`", join_path(path)),
            ));
        };
        let declared = &self.types[id.0];
        let name = declared.name.clone();
        let parameter_count = type_argument_count(declared.group(), declared.parameters.len());
        if parameter_count != arguments.len() {
            return Err(Diagnostic::new(
                reported_path,
                *span,
                format!(
                    "`{name}` takes {}, found {}",
                    crate::compiler::messages::count(parameter_count, "type argument"),
                    arguments.len()
                ),
            ));
        }
        let mut resolved = Vec::with_capacity(arguments.len());
        for argument in arguments {
            resolved.push(self.resolve_type_expr(
                argument,
                namespace,
                parameters,
                reported_path,
            )?);
        }
        Ok(Type::Named(id, resolved))
    }

    /// Resolve a `Type::Variant` path to the enum's type and the variant's
    /// index, when the path names a declared enum variant. A path with one
    /// segment is never a variant.
    pub(in crate::compiler::resolve_names) fn resolve_variant(
        &self,
        namespace: NamespaceId,
        path: &[String],
    ) -> Option<(TypeId, usize)> {
        let (variant, type_segments) = path.split_last()?;
        if type_segments.is_empty() {
            return None;
        }
        let id = self.find_type(namespace, type_segments)?;
        if !self.types[id.0].group().has_variants() {
            return None;
        }
        let TypeKind::Enum(variants) = &self.types[id.0].kind else {
            unreachable!("a group with variants is an enum");
        };
        let index = variants
            .iter()
            .position(|declared| &declared.name == variant)?;
        Some((id, index))
    }

    /// The declared type a bare name sees: the current namespace's own, then a
    /// type a name-`use` brings in.
    fn bare_type(&self, namespace: NamespaceId, name: &str) -> Option<TypeId> {
        if let Some(found) = self.namespaces[namespace.0].type_id(name) {
            return Some(found);
        }
        for used in &self.namespaces[namespace.0].uses {
            let Some(base) = self.name_use(used, name) else {
                continue;
            };
            if let Some(found) = self.namespaces[base.0].type_id(name)
                && reachable(self.types[found.0].public, base, namespace)
            {
                return Some(found);
            }
        }
        None
    }

    /// Find a declared type by a path. A bare single segment reads the current
    /// namespace and the names it `use`s; a qualified path starts at a tree
    /// name and walks from the root, or at a namespace-`use`'s qualifier and
    /// walks from there. A type reached across a namespace must be `pub`.
    pub(in crate::compiler::resolve_names) fn find_type(
        &self,
        namespace: NamespaceId,
        path: &[String],
    ) -> Option<TypeId> {
        let (name, prefix) = path
            .split_last()
            .expect("a type path has at least one segment");
        if prefix.is_empty() {
            return self.bare_type(namespace, name);
        }
        let target = self.namespace_of_path(namespace, prefix)?;
        let found = self.namespaces[target.0].type_id(name)?;
        reachable(self.types[found.0].public, target, namespace).then_some(found)
    }
}

/// A written type name: its path and its span. Every written name resolves
/// through one helper, which takes this.
struct WrittenType<'a> {
    path: &'a [String],
    span: &'a Span,
}
