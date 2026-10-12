//! The native call registry: every builtin the language ships.
//!
//! A row carries its namespace path, its name, the type of each parameter, and
//! its result type. The resolution stage registers natives from these rows as
//! ordinary callables with parameter nodes, so the checker treats a native and
//! a user function through one path. The runtime dispatches on the `Native`
//! id; its handler match is exhaustive, so an id without a handler does not
//! compile.
//!
//! A row's types are `Type`s, the same type every other declaration uses, so a
//! native names any type the language has. A `Type` is not
//! `const`-constructible, so the typed rows are built once per program by
//! `native_rows`, which the macro generates from the same declaration as the
//! variant and the static shape. The shape — a native's path, name, and
//! parameter count — needs no absence type and stays `const`, so the runtime
//! can check a handler's arity and name a failure without a program.
//!
//! To add a native, add one entry to `native_registry!` and one arm to
//! `Runtime::call_native`. There is no other way: the macro generates the
//! variant, the shape, and the typed row together, the checker reads the row,
//! the runtime's handler match is exhaustive, and the runtime checks the
//! handler consumes exactly the declared arity.

use crate::model::{Type, TypeId};

/// The namespace the runtime's builtins live in.
pub(crate) const BUILTIN_NAMESPACE: &[&str] = &["std"];

/// The absence type applied to `element`: `Option<element>`. The absence type
/// is the standard library's `Option`, whose id is known only once the
/// program's types are reserved.
fn option_of(absence: Option<TypeId>, element: Type) -> Type {
    Type::Named(
        absence.expect("the standard library declares the absence type"),
        vec![element],
    )
}

/// One native row: its path, name, parameter types, and result type. A native
/// with no result type produces no value; its calls are statements, exactly as
/// a kiru function with no return type. Its types are `Type`s, so a row
/// declares exactly what any other declaration does.
#[derive(Debug)]
pub(crate) struct NativeRow {
    pub(crate) path: &'static [&'static str],
    pub(crate) name: &'static str,
    pub(crate) parameters: Vec<Type>,
    pub(crate) returns: Option<Type>,
}

/// The part of a row that does not depend on the absence type: where the
/// native lives, what it is called, and how many parameters it declares. It
/// stays `const`, so the runtime can name a failure and check a handler's
/// arity, and the structural check can confirm a row exists, all without a
/// program.
#[derive(Clone, Copy, Debug)]
pub(crate) struct NativeShape {
    pub(crate) path: &'static [&'static str],
    pub(crate) name: &'static str,
    pub(crate) arity: usize,
}

/// Wrap an optional macro fragment in `Some`, or produce `None` when it is
/// absent. A row writes `-> Type` when the native produces a value and nothing
/// when its calls are statements.
macro_rules! optional {
    () => {
        None
    };
    ($value:expr) => {
        Some($value)
    };
}

/// The number of comma-separated items, as a `const` expression. A row's
/// parameter count is known without building its types, which a `const` cannot
/// do.
macro_rules! count {
    () => {
        0
    };
    ($head:expr $(, $tail:expr)*) => {
        1 + count!($($tail),*)
    };
}

/// Declare every native once: the enum variant, its static shape, its typed
/// row, and the complete lists. Because the runtime's handler match is
/// exhaustive over the variants, a native cannot exist without a row and a
/// handler.
///
/// The first argument names the absence identifier. The generated
/// `native_rows` binds it to the program's absence type id, and a row writes
/// an option type as `option_of(absence, element)`.
macro_rules! native_registry {
    ($absence:ident, $( $variant:ident { [$($segment:literal),* $(,)?], $name:literal, [$($parameter:expr),* $(,)?] $(-> $returns:expr)? } ),* $(,)?) => {
        /// A native provided by the runtime.
        #[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
        pub(crate) enum Native {
            $( $variant ),*
        }

        impl Native {
            /// Every native, in declaration order.
            #[cfg(test)]
            pub(crate) const ALL: &'static [Native] = &[ $( Native::$variant ),* ];
        }

        /// Every native's path, name, and parameter count. This needs no
        /// absence type, so it stays `const` and serves the runtime and the
        /// structural check.
        pub(crate) const NATIVE_SHAPES: &[(Native, NativeShape)] = &[
            $( (
                Native::$variant,
                NativeShape {
                    path: &[$($segment),*],
                    name: $name,
                    arity: count!($($parameter),*),
                },
            ) ),*
        ];

        /// Every native with its typed row, built once per program because a
        /// `Type` is not `const`-constructible. `absence` is the program's
        /// absence type id, which an option row names.
        pub(crate) fn native_rows($absence: Option<TypeId>) -> Vec<(Native, NativeRow)> {
            vec![
                $( (
                    Native::$variant,
                    NativeRow {
                        path: &[$($segment),*],
                        name: $name,
                        parameters: vec![ $( $parameter ),* ],
                        returns: optional!($( $returns )?),
                    },
                ) ),*
            ]
        }
    };
}

native_registry! {
    absence,
    Spawn { ["std", "process"], "spawn", [Type::Vec(Box::new(Type::Text))] -> Type::Int },
    Reap { ["std", "process"], "reap", [Type::Int] -> Type::Int },
    SignalGroup { ["std", "process"], "signal_group", [Type::Int, Type::Int] },
    Write { ["std", "io"], "write", [Type::Int, Type::Text] },
    IsTerminal { ["std", "io"], "is_terminal", [Type::Int] -> Type::Bool },
    ReadStdin { ["std", "io"], "read_stdin", [] -> Type::Text },
    Read { ["std", "fs"], "read", [Type::Text] -> Type::Text },
    WriteFile { ["std", "fs"], "write", [Type::Text, Type::Text] },
    Remove { ["std", "fs"], "remove", [Type::Text] },
    Exists { ["std", "fs"], "exists", [Type::Text] -> Type::Bool },
    Mkstemp { ["std", "fs"], "mkstemp", [Type::Text] -> Type::Text },
    List { ["std", "fs"], "list", [Type::Text] -> Type::Vec(Box::new(Type::Text)) },
    Length { ["std", "text"], "len", [Type::Text] -> Type::Int },
    Find { ["std", "text"], "find_byte", [Type::Text, Type::Text] -> option_of(absence, Type::Int) },
    Byte { ["std", "text"], "byte", [Type::Text, Type::Int] -> Type::Int },
    Slice { ["std", "text"], "slice", [Type::Text, Type::Int, Type::Int] -> Type::Text },
    SleepSeconds { ["std", "time"], "sleep_seconds", [Type::Int] },
    EnvVar { ["std", "env"], "var", [Type::Text] -> option_of(absence, Type::Text) },
    CurrentDir { ["std", "env"], "current_dir", [] -> Type::Text },
}

/// The shape of a native: its path, name, and parameter count.
pub(crate) fn native_shape(native: Native) -> &'static NativeShape {
    NATIVE_SHAPES
        .iter()
        .find(|(id, _)| *id == native)
        .map(|(_, shape)| shape)
        .expect("every native has exactly one shape")
}

/// The number of parameters a native declares. The runtime checks that a
/// handler consumes exactly this many arguments, so a handler cannot drift
/// from its row.
pub(crate) fn native_arity(native: Native) -> usize {
    native_shape(native).arity
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_native_id_has_exactly_one_row() {
        for native in Native::ALL {
            let rows = NATIVE_SHAPES.iter().filter(|(id, _)| id == native).count();
            assert_eq!(rows, 1, "{native:?} must have exactly one row");
        }
        assert_eq!(NATIVE_SHAPES.len(), Native::ALL.len());
    }

    #[test]
    fn native_paths_and_names_are_unique() {
        for (index, (_, row)) in NATIVE_SHAPES.iter().enumerate() {
            for (_, other) in NATIVE_SHAPES.iter().skip(index + 1) {
                assert_ne!(
                    (row.path, row.name),
                    (other.path, other.name),
                    "natives `{}` and `{}` share a path",
                    row.name,
                    other.name
                );
            }
        }
    }

    #[test]
    fn natives_live_under_the_builtin_namespace() {
        for (native, row) in NATIVE_SHAPES {
            assert!(
                row.path.first().copied() == Some(BUILTIN_NAMESPACE[0]),
                "native {native:?} is outside the builtin namespace"
            );
        }
    }
}
