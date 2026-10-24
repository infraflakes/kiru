//! The groups a type can belong to, and every fact each group declares.
//!
//! A group is the kind of thing a type is: a scalar, a declared `rec`, a
//! declared `enum`, a function, or a type parameter. Every fact about a kind is
//! declared here, once, as one match over `Group` with an arm for every group
//! and no wildcard: a group that is not handled fails to compile, so a kind can
//! never fall through to a silent default. The marking in `types.rs` and the
//! relations in `relation.rs` read these facts instead of matching a type's own
//! variants, so a new kind is described in this one place.

use super::super::declarations::{RuntimeType, TypeDecl, TypeKind};
use super::Type;
use super::TypeId;

/// The kind of thing a type is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Group {
    Scalar,
    Rec,
    Enum,
    Function,
    Parameter,
}

/// One named type a group declares: the word a written type uses, the word a
/// diagnostic shows, and the type it denotes. A scalar declares `txt`, `bool`,
/// and `int`; every other group declares none, because a declared type takes
/// its name from its declaration, a function type is written structurally, and
/// a type parameter takes the name it was declared with.
pub(crate) struct GroupName {
    pub(crate) written: &'static str,
    pub(crate) display: &'static str,
    pub(crate) ty: Type,
    /// The values the scalar can take, when they are a finite set a match may
    /// enumerate. `txt` and `int` are open.
    pub(crate) finite: Finite,
}

/// The values a subject can take, when it is a finite set a match can cover.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Finite {
    /// No finite set: only a `_` arm or an atom that covers the value decides
    /// exhaustiveness.
    Open,
    /// The boolean values, `false` and `true`, in that order.
    Booleans,
    /// The declaration's variants, by index.
    Variants,
}

/// Every scalar's name: one table, one spelling. The written word and the
/// displayed word are the same word, so a diagnostic never names a scalar
/// differently from the way a program writes it.
const SCALAR_NAMES: &[GroupName] = &[
    GroupName {
        written: "txt",
        display: "txt",
        ty: Type::Text,
        finite: Finite::Open,
    },
    GroupName {
        written: "bool",
        display: "bool",
        ty: Type::Bool,
        finite: Finite::Booleans,
    },
    GroupName {
        written: "int",
        display: "int",
        ty: Type::Int,
        finite: Finite::Open,
    },
];

/// How many type arguments a type of a group takes.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Arity {
    /// A fixed count: a scalar, a function type, and a parameter take none.
    Fixed(usize),
    /// As many as the declaration's type parameters: a `rec` or an `enum`.
    Declared,
}

/// The shape of value literal a kind is written as.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Shape {
    /// A literal word: `"…"`, `1`, `true`.
    Word,
    /// A `{ … }` record literal.
    Record,
    /// A `Type.Variant(…)` construction.
    Variant,
    /// A `[…] { … }` literal.
    Callable,
    /// A type parameter: no literal of its own.
    Parameter,
    /// No literal form: the value is built and changed through the type's own
    /// operations. The runtime list declares this.
    None,
}

/// How a kind's type is substituted: what `substitute` does at each node.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Substitution {
    /// A concrete leaf: a scalar passes through unchanged.
    Leaf,
    /// An applied type: recurse into its arguments.
    Applied,
    /// A function type: recurse into its parameters and return type.
    Callable,
    /// A type parameter: replace it from the binding.
    Parameter,
}

/// How a kind's type unifies: what `unify` compares at each node.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Unification {
    /// A leaf: equal to the actual type.
    Equality,
    /// An applied type: the same declaration and unified arguments.
    Applied,
    /// A function type: the same arity and unified parts.
    Callable,
    /// A type parameter: bound in a solution, or a literal name that must
    /// equal the actual type.
    Parameter,
}

/// The runtime representation a kind is stored as.
///
/// This is the one enumeration of the runtime's kinds: the runtime's own
/// values carry one of these and read it instead of re-deriving a kind, so
/// `Value` invents nothing beside this declaration. The scalars are separate
/// shapes, because an operation applies to exactly one scalar and no other;
/// the `rec` and `enum` groups separate their shapes, so the runtime reads a
/// list, a record, and a variant as three declared facts and not one object
/// it splits by hand.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum RuntimeShape {
    /// The text scalar.
    Text,
    /// The boolean scalar.
    Bool,
    /// The integer scalar.
    Int,
    /// A `rec` built from its fields: the record the program owns.
    Record,
    /// The one `rec` the runtime represents itself: the list, built and
    /// changed only through its own operations.
    List,
    /// An `enum` construction: its declared variant tag and payload.
    Variant,
    /// A function value: its lowered code and the values it captured.
    Function,
    /// The absence: the one shape a call that declares no return produces.
    /// It is not a value of any type; a register only holds it before it is
    /// written or after a discarded call.
    Nothing,
    /// A type parameter: it has no runtime value of its own.
    Parameter,
}

impl Group {
    /// The named types this group declares.
    pub(crate) fn names(self) -> &'static [GroupName] {
        match self {
            Group::Scalar => SCALAR_NAMES,
            Group::Rec => &[],
            Group::Enum => &[],
            Group::Function => &[],
            Group::Parameter => &[],
        }
    }

    /// How many type arguments a type of this group takes.
    pub(crate) fn arity(self) -> Arity {
        match self {
            Group::Scalar => Arity::Fixed(0),
            Group::Rec => Arity::Declared,
            Group::Enum => Arity::Declared,
            Group::Function => Arity::Fixed(0),
            Group::Parameter => Arity::Fixed(0),
        }
    }

    /// Whether methods may bind to a type of this group.
    pub(crate) fn binds_methods(self) -> bool {
        match self {
            Group::Scalar => true,
            Group::Rec => true,
            Group::Enum => true,
            Group::Function => false,
            Group::Parameter => false,
        }
    }

    /// Whether a type of this group carries fields.
    pub(crate) fn has_fields(self) -> bool {
        match self {
            Group::Scalar => false,
            Group::Rec => true,
            Group::Enum => false,
            Group::Function => false,
            Group::Parameter => false,
        }
    }

    /// Whether a type of this group carries variants.
    pub(crate) fn has_variants(self) -> bool {
        match self {
            Group::Scalar => false,
            Group::Rec => false,
            Group::Enum => true,
            Group::Function => false,
            Group::Parameter => false,
        }
    }

    /// The shape of value literal a type of this group is written as.
    pub(crate) fn shape(self) -> Shape {
        match self {
            Group::Scalar => Shape::Word,
            Group::Rec => Shape::Record,
            Group::Enum => Shape::Variant,
            Group::Function => Shape::Callable,
            Group::Parameter => Shape::Parameter,
        }
    }

    /// How a type of this group is substituted.
    pub(crate) fn substitution(self) -> Substitution {
        match self {
            Group::Scalar => Substitution::Leaf,
            Group::Rec => Substitution::Applied,
            Group::Enum => Substitution::Applied,
            Group::Function => Substitution::Callable,
            Group::Parameter => Substitution::Parameter,
        }
    }

    /// How a type of this group unifies.
    pub(crate) fn unification(self) -> Unification {
        match self {
            Group::Scalar => Unification::Equality,
            Group::Rec => Unification::Applied,
            Group::Enum => Unification::Applied,
            Group::Function => Unification::Callable,
            Group::Parameter => Unification::Parameter,
        }
    }
}

impl TypeDecl {
    /// The group a declared type belongs to: a `rec` is `Rec`, an `enum` is
    /// `Enum`. This is the one place a declaration's kind becomes a group, so
    /// every `Type::Named` reads its group from here.
    pub(crate) fn group(&self) -> Group {
        match self.kind {
            TypeKind::Rec(_) => Group::Rec,
            TypeKind::Enum(_) => Group::Enum,
        }
    }

    /// The shape of value literal this declaration is written as. The group
    /// declares the default and the runtime's marker overrides it: the list
    /// has no literal form, because the runtime builds it through its own
    /// operations.
    pub(crate) fn shape(&self) -> Shape {
        match self.runtime {
            RuntimeType::Declared => self.group().shape(),
            RuntimeType::List => Shape::None,
        }
    }

    /// The runtime representation a value of this declaration is stored as.
    /// The kind's group declares the default and the runtime's marker
    /// overrides it, exactly as the literal shape above does: the list is the
    /// one declared type whose values the runtime keeps for itself.
    pub(crate) fn runtime_shape(&self) -> RuntimeShape {
        match self.runtime {
            RuntimeType::Declared => match self.kind {
                TypeKind::Rec(_) => RuntimeShape::Record,
                TypeKind::Enum(_) => RuntimeShape::Variant,
            },
            RuntimeType::List => RuntimeShape::List,
        }
    }

    /// Whether the runtime represents this type itself.
    pub(crate) fn is_list(&self) -> bool {
        self.runtime == RuntimeType::List
    }

    /// The element parameter of the runtime list, by the spelling its
    /// declaration gave it.
    pub(crate) fn element_parameter(&self) -> Option<&str> {
        if !self.is_list() {
            return None;
        }
        self.parameters.first().map(String::as_str)
    }

    /// The values this declaration can take, when it declares a finite set.
    pub(crate) fn finite(&self) -> Finite {
        match self.group() {
            Group::Enum => Finite::Variants,
            Group::Rec => Finite::Open,
            Group::Scalar | Group::Function | Group::Parameter => Finite::Open,
        }
    }
}

impl Type {
    /// The runtime shape a value of this type is stored as: one reading of the
    /// shape set declared above. A scalar's shape is one value of the shape
    /// set rather than the group's whole family, because an operation applies
    /// to exactly one scalar; a declared type reads its declaration, and a
    /// function type and a type parameter are their own.
    pub(crate) fn runtime_shape(&self, types: &[TypeDecl]) -> RuntimeShape {
        match self {
            Type::Text => RuntimeShape::Text,
            Type::Bool => RuntimeShape::Bool,
            Type::Int => RuntimeShape::Int,
            Type::Named(id, _) => types[id.0].runtime_shape(),
            Type::Function { .. } => RuntimeShape::Function,
            Type::Parameter(_) => RuntimeShape::Parameter,
        }
    }
}

/// The values a subject can take. A scalar's name declares whether it is a
/// finite set; a declared type's group says whether its declaration enumerates
/// its values. Every caller that covers a subject reads this one fact.
pub(crate) fn finite(ty: &Type, types: &[TypeDecl]) -> Finite {
    match ty.group(types) {
        Group::Scalar => Group::Scalar
            .names()
            .iter()
            .find(|named| named.ty == *ty)
            .map_or(Finite::Open, |named| named.finite),
        Group::Rec | Group::Function | Group::Parameter => Finite::Open,
        Group::Enum => {
            let Type::Named(id, _) = ty else {
                unreachable!("an enum is a named type");
            };
            types[id.0].finite()
        }
    }
}

/// The id of the declaration the runtime represents itself, when the program
/// has one. The marker lives on the declaration, so this reads it rather than
/// any stage carrying the id.
pub(crate) fn list_type(types: &[TypeDecl]) -> Option<TypeId> {
    types
        .iter()
        .position(|declaration| declaration.is_list())
        .map(TypeId)
}

/// The scalar type a written name denotes, or `None` when the runtime provides
/// no scalar by that name. The scalar group's name table is the only place a
/// written scalar word is spelled.
pub(crate) fn scalar_from_name(name: &str) -> Option<Type> {
    Group::Scalar
        .names()
        .iter()
        .find(|named| named.written == name)
        .map(|named| named.ty.clone())
}

/// The displayed name of a type that declares one: a scalar's word. `None` for
/// every type that takes its name from elsewhere.
pub(crate) fn display_name(ty: &Type) -> Option<&'static str> {
    Group::Scalar
        .names()
        .iter()
        .find(|named| named.ty == *ty)
        .map(|named| named.display)
}

/// The name of a type for a diagnostic: a declared type shows its declared name
/// and arguments, a parameter shows its name, a scalar shows its word, and a
/// function type shows its written form. The rendering is a fact of each group,
/// so a new kind declares how it is shown beside every other fact.
pub(crate) fn type_name(ty: &Type, types: &[TypeDecl]) -> String {
    match ty.group(types).substitution() {
        Substitution::Applied => {
            let Type::Named(id, arguments) = ty else {
                unreachable!("an applied type is a named type");
            };
            named_type_name(&types[id.0].name, arguments, types)
        }
        Substitution::Leaf => display_name(ty)
            .expect("a leaf has a declared name")
            .to_owned(),
        Substitution::Callable => {
            let Type::Function {
                parameters,
                returns,
            } = ty
            else {
                unreachable!("a callable group is a function type");
            };
            let parts: Vec<String> = parameters
                .iter()
                .map(|parameter| type_name(parameter, types))
                .collect();
            // A function with no return type has no `->` in its written form,
            // because it has no return type to write.
            match returns {
                Some(returns) => {
                    format!("fn({}) -> {}", parts.join(", "), type_name(returns, types))
                }
                None => format!("fn({})", parts.join(", ")),
            }
        }
        Substitution::Parameter => {
            let Type::Parameter(name) = ty else {
                unreachable!("a parameter group is a parameter type");
            };
            name.clone()
        }
    }
}

/// The name of an applied declared type: its declared name, with its arguments
/// in `<>` when it has any.
fn named_type_name(name: &str, arguments: &[Type], types: &[TypeDecl]) -> String {
    if arguments.is_empty() {
        return name.to_owned();
    }
    let parts: Vec<String> = arguments
        .iter()
        .map(|argument| type_name(argument, types))
        .collect();
    format!("{name}<{}>", parts.join(", "))
}
