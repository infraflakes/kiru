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
    Arity, Finite, Group, RuntimeShape, Shape, display_name, finite, scalar_from_name, type_name,
};
pub(crate) use relation::{Binding, fits, has_parameter, substitute, type_parameters_of, unify};

use group::Substitution;

use crate::compiler::graph::{Declaration, DeclarationId};

/// A type.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub(crate) enum Type {
    Text,
    Bool,
    Int,
    /// A declared type applied to arguments: `Name<T1, ...>`. A declaration
    /// with no parameters is applied to no arguments. The runtime's `Vec<T>`
    /// is one such declaration, registered by the compiler.
    Named(DeclarationId, Vec<Type>),
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
    Named(DeclarationId),
    /// A function type.
    Function,
    /// A bare type parameter; it has no methods and matches no impl.
    Parameter,
}

impl Type {
    /// The group this type belongs to: the kind of thing it is. The marking is
    /// the one derivation from a type to its kind, and a declared type reads it
    /// from its declaration's type body, so `Vec` is a `Rec` and every relation
    /// that reads its group learns that in one step. `declarations` is the
    /// program's declaration table.
    pub(crate) fn group(&self, declarations: &[Declaration]) -> Group {
        match self {
            Type::Text | Type::Bool | Type::Int => Group::Scalar,
            Type::Named(id, _) => declarations[id.0]
                .type_body()
                .expect("a named type names a type declaration")
                .group(),
            Type::Function { .. } => Group::Function,
            Type::Parameter(_) => Group::Parameter,
        }
    }

    /// The head of this type, for method lookup: one match over the group's
    /// applied-type fact plus the type's identity. Dispatch keys on the
    /// specific type, so a method declared for `Vec<T>` answers a `Vec<int>`
    /// receiver and the scalars stay distinct.
    pub(crate) fn head(&self, declarations: &[Declaration]) -> TypeHead {
        match self.group(declarations).substitution() {
            Substitution::Applied => {
                let Type::Named(id, _) = self else {
                    unreachable!("an applied type is a named type");
                };
                TypeHead::Named(*id)
            }
            Substitution::Leaf => {
                TypeHead::Scalar(display_name(self).expect("a leaf has a declared name"))
            }
            Substitution::Callable => TypeHead::Function,
            Substitution::Parameter => TypeHead::Parameter,
        }
    }

    /// The return type of a function type, or `None` when it declares none or
    /// the type is not a function type. One definition serves a declaration, a
    /// function literal, and a call's result.
    pub(crate) fn return_type(&self, declarations: &[Declaration]) -> Option<Type> {
        match self.group(declarations) {
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

        // The runtime shapes the marking reads: the three scalars carry one
        // shape each, a function type and a type parameter are their own, and
        // a declared type reads its declaration.
        assert_eq!(Type::Text.runtime_shape(&[]), RuntimeShape::Text);
        assert_eq!(Type::Bool.runtime_shape(&[]), RuntimeShape::Bool);
        assert_eq!(Type::Int.runtime_shape(&[]), RuntimeShape::Int);
        assert_eq!(
            Type::Function {
                parameters: Vec::new(),
                returns: None
            }
            .runtime_shape(&[]),
            RuntimeShape::Function
        );
        assert_eq!(
            Type::Parameter("T".to_owned()).runtime_shape(&[]),
            RuntimeShape::Parameter
        );
    }

    #[test]
    fn a_declared_type_reads_the_shape_its_declaration_says() {
        let program = checked_program(
            "enum Color { Red, Green };\nrec Point { x: int };\nfn main() { let xs: std::dynamic::Vec<int> = std::dynamic::Vec.new(); return; };",
        );
        let color = program.declarations.iter().position(|declaration| {
            declaration
                .type_body()
                .is_some_and(|body| body.group() == Group::Enum)
        });
        let point = program.declarations.iter().position(|declaration| {
            declaration
                .type_body()
                .is_some_and(|body| body.group() == Group::Rec)
        });
        let list = program
            .list_type()
            .expect("the compiler registers the runtime list");
        assert_eq!(
            Type::Named(
                DeclarationId(color.expect("an enum is declared")),
                Vec::new()
            )
            .runtime_shape(&program.declarations),
            RuntimeShape::Variant
        );
        assert_eq!(
            Type::Named(DeclarationId(point.expect("a rec is declared")), Vec::new())
                .runtime_shape(&program.declarations),
            RuntimeShape::Record
        );
        assert_eq!(
            Type::Named(list, vec![Type::Int]).runtime_shape(&program.declarations),
            RuntimeShape::List
        );
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
            .list_type()
            .expect("the compiler registers the runtime list");
        let applied = Type::Named(vec, vec![Type::Int]);
        assert_eq!(applied.group(&program.declarations), Group::Rec);
        assert_eq!(applied.head(&program.declarations), TypeHead::Named(vec));
        assert_eq!(
            program.declarations[vec.0]
                .type_body()
                .expect("Vec is a type declaration")
                .group()
                .arity(),
            Arity::Declared,
            "Vec takes the declaration's one type parameter"
        );
        assert_eq!(
            program.declarations[vec.0]
                .type_body()
                .expect("Vec is a type declaration")
                .parameters
                .len(),
            1
        );
    }

    #[test]
    fn declarations_take_their_own_groups() {
        let program = checked_program(
            "rec Point { x: int }; enum Choice { a, b }; \
             fn main() { let p: Point = { x = 1 }; let c: Choice = Choice.a; return; };",
        );
        let point = program
            .declarations
            .iter()
            .position(|declaration| declaration.name == "Point")
            .expect("Point is declared");
        let choice = program
            .declarations
            .iter()
            .position(|declaration| declaration.name == "Choice")
            .expect("Choice is declared");
        assert_eq!(
            program.declarations[point]
                .type_body()
                .expect("Point is a type declaration")
                .group(),
            Group::Rec
        );
        assert_eq!(
            program.declarations[choice]
                .type_body()
                .expect("Choice is a type declaration")
                .group(),
            Group::Enum
        );
    }
}
