//! The relations over types: substitution, unification, fit, and parameter
//! containment.
//!
//! Every relation reads the group a type is marked with; none matches the
//! type's own variants to decide what to do. The group says how a kind
//! substitutes and how it unifies, so a new kind is described once, in
//! `group.rs`, and every relation follows.

use super::super::declarations::TypeDecl;
use super::Type;
use super::group::{Substitution, Unification};

/// The binding a substitution applies to a type parameter. A complete
/// substitution supplies one argument per parameter; a partial one binds only
/// the parameters a solution has settled and leaves the rest in place, so the
/// call walk can read the shape a solution has already fixed.
#[derive(Clone, Copy)]
pub(crate) enum Binding<'a> {
    Complete(&'a [Type]),
    Partial(&'a [Option<Type>]),
}

/// Replace every parameter of `parameters` in `ty` with the binding at its
/// position. A concrete type passes through unchanged; a partial binding leaves
/// an unbound parameter in place.
pub(crate) fn substitute(
    ty: &Type,
    types: &[TypeDecl],
    parameters: &[String],
    binding: Binding<'_>,
) -> Type {
    match ty.group(types).substitution() {
        Substitution::Leaf => ty.clone(),
        Substitution::Parameter => bound_parameter(ty, parameters, binding),
        Substitution::Applied => {
            let Type::Named(id, arguments) = ty else {
                unreachable!("an applied group is a named type");
            };
            Type::Named(
                *id,
                arguments
                    .iter()
                    .map(|argument| substitute(argument, types, parameters, binding))
                    .collect(),
            )
        }
        Substitution::Callable => {
            let Type::Function {
                parameters: declared,
                returns,
            } = ty
            else {
                unreachable!("a callable group is a function type");
            };
            Type::Function {
                parameters: declared
                    .iter()
                    .map(|parameter| substitute(parameter, types, parameters, binding))
                    .collect(),
                returns: returns
                    .as_ref()
                    .map(|returns| Box::new(substitute(returns, types, parameters, binding))),
            }
        }
    }
}

/// The type a parameter stands for under a binding. A complete binding supplies
/// the argument at the parameter's position; a partial one supplies the bound
/// value, or the parameter itself when it is still open.
fn bound_parameter(ty: &Type, parameters: &[String], binding: Binding<'_>) -> Type {
    let Type::Parameter(name) = ty else {
        unreachable!("a parameter group is a parameter type");
    };
    let index = parameters
        .iter()
        .position(|parameter| parameter == name)
        .expect("a type parameter is declared");
    match binding {
        Binding::Complete(arguments) => arguments[index].clone(),
        Binding::Partial(solution) => match &solution[index] {
            Some(bound) => bound.clone(),
            None => ty.clone(),
        },
    }
}

/// Whether a value of `actual` may stand where `required` is asked for. Types
/// are exact, so this is `unify` with no binders: every type parameter is a
/// literal name and every leaf is equality. It is the one compatibility
/// relation — `unify` binds a declaration's type parameters, `fits` binds none.
pub(crate) fn fits(actual: &Type, required: &Type, types: &[TypeDecl]) -> bool {
    unify(required, actual, types, &[], &mut [])
}

/// The one relation over two types: match a `declared` type (which may contain
/// type parameters) against an `actual` type, recording each parameter's
/// binding in `solution` (indexed by the parameter's position in `parameters`).
/// Returns false when the shapes do not match or a parameter is bound twice to
/// different types. With no bindable parameters — every parameter is then a
/// literal name — this is exact equality, which `fits` specializes to.
pub(crate) fn unify(
    declared: &Type,
    actual: &Type,
    types: &[TypeDecl],
    parameters: &[String],
    solution: &mut [Option<Type>],
) -> bool {
    match declared.group(types).unification() {
        Unification::Equality => declared == actual,
        Unification::Parameter => {
            let Type::Parameter(name) = declared else {
                unreachable!("a parameter group is a parameter type");
            };
            let Some(index) = parameters.iter().position(|parameter| parameter == name) else {
                // No binder names this parameter, so it is a literal type name
                // and must equal the actual type. This is the `fits` leaf.
                return declared == actual;
            };
            match &solution[index] {
                Some(bound) => bound == actual,
                None => {
                    solution[index] = Some(actual.clone());
                    true
                }
            }
        }
        Unification::Applied => {
            let Type::Named(id, arguments) = declared else {
                unreachable!("an applied group is a named type");
            };
            match actual.group(types).unification() {
                Unification::Applied => {
                    let Type::Named(other, other_arguments) = actual else {
                        unreachable!("an applied group is a named type");
                    };
                    id == other
                        && arguments.len() == other_arguments.len()
                        && arguments
                            .iter()
                            .zip(other_arguments)
                            .all(|(declared, actual)| {
                                unify(declared, actual, types, parameters, solution)
                            })
                }
                Unification::Equality | Unification::Callable | Unification::Parameter => false,
            }
        }
        Unification::Callable => {
            let Type::Function {
                parameters: declared_parameters,
                returns,
            } = declared
            else {
                unreachable!("a callable group is a function type");
            };
            match actual.group(types).unification() {
                Unification::Callable => {
                    let Type::Function {
                        parameters: actual_parameters,
                        returns: actual_returns,
                    } = actual
                    else {
                        unreachable!("a callable group is a function type");
                    };
                    declared_parameters.len() == actual_parameters.len()
                        && declared_parameters.iter().zip(actual_parameters).all(
                            |(declared, actual)| {
                                unify(declared, actual, types, parameters, solution)
                            },
                        )
                        && match (returns, actual_returns) {
                            (Some(declared), Some(actual)) => {
                                unify(declared, actual, types, parameters, solution)
                            }
                            (None, None) => true,
                            _ => false,
                        }
                }
                Unification::Equality | Unification::Applied | Unification::Parameter => false,
            }
        }
    }
}

/// Every type parameter `ty` mentions, in order of first appearance. A native's
/// row is a function type that names the parameters a call binds, and a
/// declaration without a function node reads them from here.
pub(crate) fn type_parameters_of(ty: &Type, types: &[TypeDecl]) -> Vec<String> {
    let mut found = Vec::new();
    collect_type_parameters(ty, types, &mut found);
    found
}

/// Whether `ty` mentions any type parameter. The "any?" caller of the one walk
/// that answers which parameters a type contains.
pub(crate) fn has_parameter(ty: &Type, types: &[TypeDecl]) -> bool {
    !type_parameters_of(ty, types).is_empty()
}

/// The one walk over a type that answers which parameters it contains. The walk
/// follows the group's substitution structure: a leaf holds none, a parameter
/// holds itself, an applied type holds its arguments', and a function holds its
/// parameters' and its return's.
fn collect_type_parameters(ty: &Type, types: &[TypeDecl], found: &mut Vec<String>) {
    match ty.group(types).substitution() {
        Substitution::Leaf => {}
        Substitution::Parameter => {
            let Type::Parameter(name) = ty else {
                unreachable!("a parameter group is a parameter type");
            };
            if !found.iter().any(|found| found == name) {
                found.push(name.clone());
            }
        }
        Substitution::Applied => {
            let Type::Named(_, arguments) = ty else {
                unreachable!("an applied group is a named type");
            };
            for argument in arguments {
                collect_type_parameters(argument, types, found);
            }
        }
        Substitution::Callable => {
            let Type::Function {
                parameters,
                returns,
            } = ty
            else {
                unreachable!("a callable group is a function type");
            };
            for parameter in parameters {
                collect_type_parameters(parameter, types, found);
            }
            if let Some(returns) = returns {
                collect_type_parameters(returns, types, found);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::compiler::model::{NamespaceId, TypeDecl, TypeId, TypeKind};

    use super::{Binding, Type, fits, substitute};

    /// A table of `rec` declarations, one per index, so a named type has a
    /// group to read.
    fn declared_types(count: usize) -> Vec<TypeDecl> {
        (0..count)
            .map(|index| TypeDecl {
                name: format!("T{index}"),
                public: true,
                namespace: NamespaceId(0),
                parameters: Vec::new(),
                kind: TypeKind::Rec(Vec::new()),
            })
            .collect()
    }

    #[test]
    fn a_value_fits_only_its_own_type() {
        let types = declared_types(4);
        let cases = [
            Type::Text,
            Type::Bool,
            Type::Int,
            Type::Named(TypeId(3), vec![Type::Text]),
            Type::Named(TypeId(3), vec![Type::Int]),
            Type::Parameter("T".to_owned()),
            Type::Function {
                parameters: vec![Type::Int],
                returns: Some(Box::new(Type::Text)),
            },
        ];
        for actual in &cases {
            for required in &cases {
                assert_eq!(
                    fits(actual, required, &types),
                    actual == required,
                    "{actual:?} against {required:?}"
                );
            }
        }
    }

    #[test]
    fn substitute_replaces_parameters() {
        let types = declared_types(8);
        let parameters = vec!["T".to_owned()];
        let ty = Type::Named(TypeId(7), vec![Type::Parameter("T".to_owned())]);
        assert_eq!(
            substitute(&ty, &types, &parameters, Binding::Complete(&[Type::Int])),
            Type::Named(TypeId(7), vec![Type::Int])
        );
    }
}
