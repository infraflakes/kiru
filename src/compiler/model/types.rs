//! The language's type set.
//!
//! A type is a runtime scalar (`txt`, `bool`, `int`), a declared type applied
//! to arguments (`Name<T1, ...>`, which includes the runtime's `Vec<T>`), a
//! `fn(...)` value type, or a type parameter of the enclosing declaration.
//! Types are exact: a value fits a position only when the two types are equal.
//! There is no no-value type: a function with no return type has a function
//! type whose `returns` is `None`, and a call to it produces no value at all.
//!
//! Every type is marked with a group, the kind of thing it is. The marking is
//! the one derivation from a type to its kind: `group.rs` declares what each
//! kind is, and `relation.rs` holds the relations over types. No relation
//! re-derives a kind by matching a type's own variants, and no match over a
//! group carries a wildcard, so a new kind is a compile error everywhere it
//! must be described rather than a silent default.

mod group;
mod relation;

pub(crate) use group::{
    Arity, Group, RuntimeShape, Shape, display_name, scalar_from_name, type_name,
};
pub(crate) use relation::{Binding, fits, has_parameter, substitute, type_parameters_of, unify};

use super::declarations::TypeDecl;

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

/// The type constructor of a value's type, used for method dispatch: a method
/// declared for `Vec<T>` answers a `Vec<int>` receiver, so lookup keys on the
/// head and ignores the arguments. An associated function is keyed by the head
/// of the type it is filed under. A head is the group plus the type's identity,
/// so it is derived from the marking rather than from a list of variants.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum TypeHead {
    /// A scalar, by the word that names it (`txt`, `bool`, `int`).
    Scalar(&'static str),
    /// A declared type, by its declaration.
    Named(TypeId),
    /// A function type.
    Function,
    /// A bare type parameter; it has no methods and matches no impl.
    Parameter,
}

impl Type {
    /// The group this type belongs to: the kind of thing it is. The marking is
    /// the one derivation from a type to its kind, and a declared type reads it
    /// from its declaration, so `Vec` is a `Rec` and every relation that reads
    /// its group learns that in one step. `types` is the program's declared
    /// types.
    pub(crate) fn group(&self, types: &[TypeDecl]) -> Group {
        match self {
            Type::Text | Type::Bool | Type::Int => Group::Scalar,
            Type::Named(id, _) => types[id.0].group(),
            Type::Function { .. } => Group::Function,
            Type::Parameter(_) => Group::Parameter,
        }
    }

    /// The head of this type, for method lookup: one match over the group plus
    /// the type's identity. Dispatch keys on the specific type, so a method
    /// declared for `Vec<T>` answers a `Vec<int>` receiver and the scalars stay
    /// distinct.
    pub(crate) fn head(&self, types: &[TypeDecl]) -> TypeHead {
        match self.group(types) {
            Group::Scalar => {
                TypeHead::Scalar(display_name(self).expect("a scalar has a declared name"))
            }
            Group::Rec => {
                let Type::Named(id, _) = self else {
                    unreachable!("a rec is a named type");
                };
                TypeHead::Named(*id)
            }
            Group::Enum => {
                let Type::Named(id, _) = self else {
                    unreachable!("an enum is a named type");
                };
                TypeHead::Named(*id)
            }
            Group::Function => TypeHead::Function,
            Group::Parameter => TypeHead::Parameter,
        }
    }

    /// The return type of a function type, or `None` when it declares none or
    /// the type is not a function type. One definition serves a declaration, a
    /// function literal, and a call's result.
    pub(crate) fn return_type(&self, types: &[TypeDecl]) -> Option<Type> {
        match self.group(types) {
            Group::Function => {
                let Type::Function { returns, .. } = self else {
                    unreachable!("a function group is a function type");
                };
                returns.as_ref().map(|returns| (**returns).clone())
            }
            Group::Scalar => None,
            Group::Rec => None,
            Group::Enum => None,
            Group::Parameter => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::group::{Shape, Substitution, Unification};
    use super::*;
    use crate::compiler::checked_program;

    #[test]
    fn every_group_declares_every_fact() {
        // Every group is reachable, and each fact is total: the list below has
        // an arm for all five, and a new group is a compile error in `group.rs`
        // rather than a gap. The assertions name the facts themselves.
        let groups = [
            Group::Scalar,
            Group::Rec,
            Group::Enum,
            Group::Function,
            Group::Parameter,
        ];
        assert_eq!(groups.len(), 5, "the model has exactly five groups");

        assert_eq!(Group::Scalar.names().len(), 3, "the three scalars");
        for group in groups {
            let _ = group.names();
            let _ = group.arity();
            let _ = group.binds_methods();
            let _ = group.has_fields();
            let _ = group.has_variants();
            let _ = group.shape();
            let _ = group.substitution();
            let _ = group.unification();
            let _ = group.runtime_shape();
        }

        assert_eq!(Group::Scalar.arity(), Arity::Fixed(0));
        assert_eq!(Group::Rec.arity(), Arity::Declared);
        assert_eq!(Group::Enum.arity(), Arity::Declared);
        assert_eq!(Group::Function.arity(), Arity::Fixed(0));
        assert_eq!(Group::Parameter.arity(), Arity::Fixed(0));

        assert!(Group::Scalar.binds_methods());
        assert!(Group::Rec.binds_methods());
        assert!(Group::Enum.binds_methods());
        assert!(!Group::Function.binds_methods());
        assert!(!Group::Parameter.binds_methods());

        assert!(!Group::Scalar.has_fields());
        assert!(Group::Rec.has_fields());
        assert!(!Group::Enum.has_fields());
        assert!(!Group::Function.has_fields());
        assert!(!Group::Parameter.has_fields());

        assert!(!Group::Scalar.has_variants());
        assert!(!Group::Rec.has_variants());
        assert!(Group::Enum.has_variants());
        assert!(!Group::Function.has_variants());
        assert!(!Group::Parameter.has_variants());

        assert_eq!(Group::Scalar.shape(), Shape::Word);
        assert_eq!(Group::Rec.shape(), Shape::Record);
        assert_eq!(Group::Enum.shape(), Shape::Variant);
        assert_eq!(Group::Function.shape(), Shape::Callable);
        assert_eq!(Group::Parameter.shape(), Shape::Parameter);

        assert_eq!(Group::Scalar.substitution(), Substitution::Leaf);
        assert_eq!(Group::Rec.substitution(), Substitution::Applied);
        assert_eq!(Group::Enum.substitution(), Substitution::Applied);
        assert_eq!(Group::Function.substitution(), Substitution::Callable);
        assert_eq!(Group::Parameter.substitution(), Substitution::Parameter);

        assert_eq!(Group::Scalar.unification(), Unification::Equality);
        assert_eq!(Group::Rec.unification(), Unification::Applied);
        assert_eq!(Group::Enum.unification(), Unification::Applied);
        assert_eq!(Group::Function.unification(), Unification::Callable);
        assert_eq!(Group::Parameter.unification(), Unification::Parameter);

        assert_eq!(Group::Scalar.runtime_shape(), RuntimeShape::Scalar);
        assert_eq!(Group::Rec.runtime_shape(), RuntimeShape::Object);
        assert_eq!(Group::Enum.runtime_shape(), RuntimeShape::Object);
        assert_eq!(Group::Function.runtime_shape(), RuntimeShape::Function);
        assert_eq!(Group::Parameter.runtime_shape(), RuntimeShape::Parameter);
    }

    #[test]
    fn written_and_display_names_agree_for_every_scalar() {
        for named in Group::Scalar.names() {
            assert_eq!(
                named.written, named.display,
                "a scalar is written and shown by one spelling"
            );
            assert_eq!(
                scalar_from_name(named.written),
                Some(named.ty.clone()),
                "a scalar resolves from its written word"
            );
            assert_eq!(
                display_name(&named.ty),
                Some(named.written),
                "a scalar shows its written word"
            );
        }
        assert_eq!(scalar_from_name("text"), None, "`text` is not a spelling");
        assert_eq!(scalar_from_name("Vec"), None, "`Vec` is a declaration");
    }

    #[test]
    fn vec_is_a_rec_and_is_dispatchable() {
        let program = checked_program(
            "fn main() { let xs: std::dynamic::Vec<int> = std::dynamic::Vec.new(); return; };",
        );
        let vec = program
            .vec_type
            .expect("the compiler registers the Vec type");
        let applied = Type::Named(vec, vec![Type::Int]);
        assert_eq!(applied.group(&program.types), Group::Rec);
        assert_eq!(applied.head(&program.types), TypeHead::Named(vec));
        assert_eq!(
            program.types[vec.0].group().arity(),
            Arity::Declared,
            "Vec takes the declaration's one type parameter"
        );
        assert_eq!(program.types[vec.0].parameters.len(), 1);
    }

    #[test]
    fn declarations_take_their_own_groups() {
        let program = checked_program(
            "rec Point { x: int }; enum Choice { a, b }; \
             fn main() { let p: Point = { x = 1 }; let c: Choice = Choice.a; return; };",
        );
        let point = program
            .types
            .iter()
            .position(|declaration| declaration.name == "Point")
            .expect("Point is declared");
        let choice = program
            .types
            .iter()
            .position(|declaration| declaration.name == "Choice")
            .expect("Choice is declared");
        assert_eq!(program.types[point].group(), Group::Rec);
        assert_eq!(program.types[choice].group(), Group::Enum);
    }
}
