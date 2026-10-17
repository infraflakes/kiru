//! The language's type set.
//!
//! A type is a runtime scalar (`txt`, `bool`, `int`), a declared type applied
//! to arguments (`Name<T1, ...>`, which includes the runtime's `Vec<T>`), a
//! `fn(...)` value type, or a type parameter of the enclosing declaration.
//! Types are exact: a value fits a position only when the two types are equal.
//! There is no no-value type: a function with no return type has a function
//! type whose `returns` is `None`, and a call to it produces no value at all.

/// The identity of a declared type, indexing the program's type table.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct TypeId(pub(crate) usize);

/// A type.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Type {
    Text,
    Bool,
    Int,
    /// A declared type applied to arguments: `Name<T1, ...>`. A declaration
    /// with no parameters is applied to no arguments. The runtime's `Vec<T>`
    /// is one such declaration, registered by the compiler.
    Named(TypeId, Vec<Type>),
    /// A type parameter of the enclosing declaration, by name (for example `T`
    /// in `enum Option<T>`).
    Parameter(String),
    /// A function value's type: its parameter types and its return type. A
    /// `None` return is a function whose calls produce no value; it is not a
    /// type, so no type names it.
    Function {
        parameters: Vec<Type>,
        returns: Option<Box<Type>>,
    },
}

impl Type {
    /// The name of a runtime representation, for a diagnostic that cannot reach
    /// the program. A declared type and a parameter have no built-in name:
    /// every diagnostic that can name one goes through `Program::type_name`.
    pub(crate) fn builtin_name(&self) -> &'static str {
        match self {
            Type::Text => "text",
            Type::Bool => "bool",
            Type::Int => "int",
            Type::Named(..) | Type::Parameter(_) | Type::Function { .. } => {
                unreachable!("a declared type has no built-in name; use Program::type_name")
            }
        }
    }
}

/// The runtime representation a built-in type name denotes.
///
/// `txt`, `bool`, and `int` are the scalars the runtime ships. They are the
/// language's own types, not declarations: the compiler resolves the name
/// directly to this marker, which says which representation the runtime
/// provides and how many type parameters the name takes. `Vec<T>` is not here:
/// it is an ordinary declared type in the `std::dynamic` namespace.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum BuiltinType {
    Text,
    Bool,
    Int,
}

impl BuiltinType {
    /// The marker a written name denotes, or `None` when the runtime provides
    /// no type by that name.
    pub(crate) fn from_name(name: &str) -> Option<Self> {
        match name {
            "txt" => Some(BuiltinType::Text),
            "bool" => Some(BuiltinType::Bool),
            "int" => Some(BuiltinType::Int),
            _ => None,
        }
    }

    /// The name a written type uses for this marker. Production code names a
    /// runtime type through `Type::builtin_name`; this is the test-only
    /// inverse of `from_name`.
    #[cfg(test)]
    pub(crate) fn name(self) -> &'static str {
        match self {
            BuiltinType::Text => "txt",
            BuiltinType::Bool => "bool",
            BuiltinType::Int => "int",
        }
    }

    /// How many type parameters the declaration must take.
    pub(crate) fn parameter_count(self) -> usize {
        match self {
            BuiltinType::Text | BuiltinType::Bool | BuiltinType::Int => 0,
        }
    }

    /// The runtime type this marker denotes. The marker's parameter count is
    /// checked before this is called, so a scalar has no arguments.
    pub(crate) fn runtime_type(self) -> Type {
        match self {
            BuiltinType::Text => Type::Text,
            BuiltinType::Bool => Type::Bool,
            BuiltinType::Int => Type::Int,
        }
    }
}

/// Replace every parameter of `parameters` in `ty` with the argument at its
/// position. A concrete type passes through unchanged.
pub(crate) fn substitute(ty: &Type, parameters: &[String], arguments: &[Type]) -> Type {
    match ty {
        Type::Parameter(name) => {
            let index = parameters
                .iter()
                .position(|parameter| parameter == name)
                .expect("a type parameter is declared");
            arguments[index].clone()
        }
        Type::Named(id, args) => Type::Named(
            *id,
            args.iter()
                .map(|arg| substitute(arg, parameters, arguments))
                .collect(),
        ),
        Type::Function {
            parameters: declared,
            returns,
        } => Type::Function {
            parameters: declared
                .iter()
                .map(|parameter| substitute(parameter, parameters, arguments))
                .collect(),
            returns: returns
                .as_ref()
                .map(|returns| Box::new(substitute(returns, parameters, arguments))),
        },
        other => other.clone(),
    }
}

/// Replace every parameter of `parameters` that `solution` has bound, leaving
/// an unbound parameter in place. The call walk reads the shape of a parameter
/// a partial solution has settled, to pass it to an argument as the expected
/// type. A concrete type passes through unchanged.
pub(crate) fn substitute_optional(
    ty: &Type,
    parameters: &[String],
    solution: &[Option<Type>],
) -> Type {
    match ty {
        Type::Parameter(name) => {
            let index = parameters
                .iter()
                .position(|parameter| parameter == name)
                .expect("a type parameter is declared");
            match &solution[index] {
                Some(bound) => bound.clone(),
                None => ty.clone(),
            }
        }
        Type::Named(id, args) => Type::Named(
            *id,
            args.iter()
                .map(|arg| substitute_optional(arg, parameters, solution))
                .collect(),
        ),
        Type::Function {
            parameters: declared,
            returns,
        } => Type::Function {
            parameters: declared
                .iter()
                .map(|parameter| substitute_optional(parameter, parameters, solution))
                .collect(),
            returns: returns
                .as_ref()
                .map(|returns| Box::new(substitute_optional(returns, parameters, solution))),
        },
        other => other.clone(),
    }
}

/// Whether a value of `actual` may stand where `required` is asked for. Types
/// are exact, so this is `unify` with no binders: every type parameter is a
/// literal name and every leaf is equality. It is the one compatibility relation
/// — `unify` binds a declaration's type parameters, `fits` binds none.
pub(crate) fn fits(actual: &Type, required: &Type) -> bool {
    unify(required, actual, &[], &mut [])
}

/// The type constructor of a value's type, used for method dispatch: a method
/// declared for `Vec<T>` answers a `Vec<int>` receiver, so lookup keys on the
/// head and ignores the arguments. A constructor is keyed the same way, by the
/// head of the type it returns.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum TypeHead {
    Text,
    Int,
    Bool,
    Named(TypeId),
    Function,
    /// A bare type parameter; it has no methods and matches no impl.
    Parameter,
}

impl Type {
    /// The head of this type, for method lookup.
    pub(crate) fn head(&self) -> TypeHead {
        match self {
            Type::Text => TypeHead::Text,
            Type::Int => TypeHead::Int,
            Type::Bool => TypeHead::Bool,
            Type::Named(id, _) => TypeHead::Named(*id),
            Type::Function { .. } => TypeHead::Function,
            Type::Parameter(_) => TypeHead::Parameter,
        }
    }

    /// The return type of a function type, or `None` when it declares none or
    /// the type is not a function type. One definition serves a declaration, a
    /// function literal, and a call's result.
    pub(crate) fn return_type(&self) -> Option<Type> {
        match self {
            Type::Function { returns, .. } => returns.as_ref().map(|returns| (**returns).clone()),
            _ => None,
        }
    }
}
pub(crate) fn has_parameter(ty: &Type) -> bool {
    match ty {
        Type::Parameter(_) => true,
        Type::Named(_, arguments) => arguments.iter().any(has_parameter),
        Type::Function {
            parameters,
            returns,
        } => {
            parameters.iter().any(has_parameter)
                || returns
                    .as_ref()
                    .is_some_and(|returns| has_parameter(returns))
        }
        _ => false,
    }
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
    parameters: &[String],
    solution: &mut [Option<Type>],
) -> bool {
    match declared {
        Type::Parameter(name) => {
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
        Type::Named(id, arguments) => match actual {
            Type::Named(other, other_arguments) => {
                id == other
                    && arguments.len() == other_arguments.len()
                    && arguments
                        .iter()
                        .zip(other_arguments)
                        .all(|(declared, actual)| unify(declared, actual, parameters, solution))
            }
            _ => false,
        },
        Type::Function {
            parameters: declared,
            returns,
        } => match actual {
            Type::Function {
                parameters: actual_parameters,
                returns: actual_returns,
            } => {
                declared.len() == actual_parameters.len()
                    && declared
                        .iter()
                        .zip(actual_parameters)
                        .all(|(declared, actual)| unify(declared, actual, parameters, solution))
                    && match (returns, actual_returns) {
                        (Some(declared), Some(actual)) => {
                            unify(declared, actual, parameters, solution)
                        }
                        (None, None) => true,
                        _ => false,
                    }
            }
            _ => false,
        },
        other => other == actual,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_builtin_has_a_name() {
        for ty in [Type::Text, Type::Bool, Type::Int] {
            assert!(!ty.builtin_name().is_empty(), "{ty:?} has no name");
        }
    }

    #[test]
    fn builtin_names_and_arity_round_trip() {
        for (name, parameters) in [("txt", 0), ("bool", 0), ("int", 0)] {
            let builtin = BuiltinType::from_name(name).expect("a built-in name");
            assert_eq!(builtin.name(), name);
            assert_eq!(builtin.parameter_count(), parameters);
        }
        assert_eq!(BuiltinType::from_name("Missing"), None);
        assert_eq!(BuiltinType::from_name("Vec"), None);
    }

    #[test]
    fn a_value_fits_only_its_own_type() {
        let types = [
            Type::Text,
            Type::Bool,
            Type::Int,
            Type::Named(TypeId(3), vec![Type::Text]),
            Type::Named(TypeId(3), vec![Type::Int]),
        ];
        for actual in &types {
            for required in &types {
                assert_eq!(fits(actual, required), actual == required, "{actual:?}");
            }
        }
    }

    #[test]
    fn substitute_replaces_parameters() {
        let parameters = vec!["T".to_owned()];
        let ty = Type::Named(TypeId(7), vec![Type::Parameter("T".to_owned())]);
        assert_eq!(
            substitute(&ty, &parameters, &[Type::Int]),
            Type::Named(TypeId(7), vec![Type::Int])
        );
    }
}
