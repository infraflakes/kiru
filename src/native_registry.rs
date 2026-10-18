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
//! native names any type the language has. A row may be generic in the vec's
//! element: it writes `Type::Parameter`, and a call binds the parameter through
//! `unify`, exactly as a declared generic function's parameters are bound.
//! A `Type` is not `const`-constructible, so the typed rows are built once per
//! program by `native_rows`, which the macro generates from the same
//! declaration as the variant and the static shape. The shape — a native's
//! path, name, and parameter count — needs no absence type and stays `const`,
//! so the runtime can check a handler's arity and name a failure without a
//! program.
//!
//! To add a native, add one entry to `native_registry!` and one arm to
//! `Runtime::call_native`. There is no other way: the macro generates the
//! variant, the shape, and the typed row together, the checker reads the row,
//! the runtime's handler match is exhaustive, and the runtime checks the
//! handler consumes exactly the declared arity.

use crate::compiler::model::{Type, TypeId};

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

/// The runtime's `Vec` type applied to `element`. The compiler registers the
/// declaration, whose id is known only once it is registered.
fn vec_of(vec: Option<TypeId>, element: Type) -> Type {
    Type::Named(
        vec.expect("the compiler registers the Vec type"),
        vec![element],
    )
}

/// The element type parameter a vec row writes: the `T` of `Vec<T>`. A call
/// binds it through the receiver, the same `unify` every declaration uses, so
/// the row spells it as the `Type::Parameter` any other declaration writes.
fn element() -> Type {
    Type::Parameter("T".to_owned())
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
/// The first two arguments name the absence and `Vec` identifiers. The
/// generated `native_rows` binds them to the program's absence type id and the
/// compiler's `Vec` type id, and a row writes those types as
/// `option_of(absence, element)` and `vec_of(vec, element)`.
macro_rules! native_registry {
    ($absence:ident, $vec:ident, $( $variant:ident { [$($segment:literal),* $(,)?], $name:literal, [$($parameter:expr),* $(,)?] $(-> $returns:expr)? } ),* $(,)?) => {
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
        /// absence type id, which an option row names, and `vec` is the
        /// compiler's `Vec` type id, which a row over a vec names.
        pub(crate) fn native_rows($absence: Option<TypeId>, $vec: Option<TypeId>) -> Vec<(Native, NativeRow)> {
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
    vec,
    // The pure vec operations stay in `std::dynamic`.
    VecNew { ["std", "dynamic"], "new", [] -> vec_of(vec, element()) },
    VecPush { ["std", "dynamic"], "push", [vec_of(vec, element()), element()] -> vec_of(vec, element()) },
    VecReplace { ["std", "dynamic"], "replace", [vec_of(vec, element()), Type::Int, element()] -> vec_of(vec, element()) },
    VecAt { ["std", "dynamic"], "at", [vec_of(vec, element()), Type::Int] -> element() },
    VecLen { ["std", "dynamic"], "len", [vec_of(vec, element())] -> Type::Int },
    // Reading and writing a path are file operations, not syscalls, so they
    // live beside the rest of `std::fs`.
    Read { ["std", "fs"], "read", [Type::Text] -> Type::Text },
    WriteFile { ["std", "fs"], "write", [Type::Text, Type::Text] },
    // The pure byte kernels operate on text and touch no OS, so they stay
    // beside their kind under their libc names.
    Strlen { ["std", "text"], "strlen", [Type::Text] -> Type::Int },
    Memchr { ["std", "text"], "memchr", [Type::Text, Type::Text] -> option_of(absence, Type::Int) },
    Substr { ["std", "text"], "substr", [Type::Text, Type::Int, Type::Int] -> Type::Text },
    ByteAt { ["std", "text"], "byte_at", [Type::Text, Type::Int] -> Type::Int },
    // Every native that touches the OS lives in `std::syscalls`, under its
    // syscall-like name.
    Write { ["std", "syscalls"], "write", [Type::Int, Type::Text] },
    Unlink { ["std", "syscalls"], "unlink", [Type::Text] },
    Access { ["std", "syscalls"], "access", [Type::Text] -> Type::Bool },
    Mkstemp { ["std", "syscalls"], "mkstemp", [Type::Text] -> Type::Text },
    Readdir { ["std", "syscalls"], "readdir", [Type::Text] -> vec_of(vec, Type::Text) },
    Getenv { ["std", "syscalls"], "getenv", [Type::Text] -> option_of(absence, Type::Text) },
    Getcwd { ["std", "syscalls"], "getcwd", [] -> Type::Text },
    Isatty { ["std", "syscalls"], "isatty", [Type::Int] -> Type::Bool },
    ReadStdin { ["std", "syscalls"], "read_stdin", [] -> Type::Text },
    PosixSpawn { ["std", "syscalls"], "posix_spawn", [vec_of(vec, Type::Text)] -> Type::Int },
    Waitpid { ["std", "syscalls"], "waitpid", [Type::Int] -> Type::Int },
    Killpg { ["std", "syscalls"], "killpg", [Type::Int, Type::Int] },
    Nanosleep { ["std", "syscalls"], "nanosleep", [Type::Int] },
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
        // A name is unique in its namespace: two natives may never share a
        // path and a name, whatever their parameters. The descriptor `write`
        // and the path `write` are told apart by their namespaces, not their
        // heads.
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

    #[test]
    fn every_native_lives_in_a_directory_under_std() {
        let std_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("std");
        for (native, row) in NATIVE_SHAPES {
            let mut directory = std_root.clone();
            for segment in &row.path[1..] {
                directory.push(segment);
            }
            assert!(
                directory.is_dir(),
                "native {native:?} lives in `{}`, which is not a directory under `std/`",
                row.path.join("::")
            );
        }
    }
}
