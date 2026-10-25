//! The native call registry: every builtin the language ships.
//!
//! A row carries its namespace path, its name, the type of each parameter, and
//! its result type. The resolution stage registers natives from these rows as
//! ordinary callables with parameter nodes, so the checker treats a native and
//! a user function through one path. The runtime dispatches on the `Native`
//! id; its handler match is exhaustive, so an id without a handler does not
//! compile.
//!
//! A row's types name what the native takes and returns by the names the
//! language uses. A row that names a declared type writes its absolute path
//! and the resolution stage resolves it through the ordinary type resolver,
//! exactly as a written annotation does; a row that names the list's element
//! writes the list's own parameter, so its declaration is the one spelling.
//! A `Type` is not `const`-constructible, so the typed rows are built once per
//! program by `native_rows`, which the macro generates from the same
//! declaration as the variant and the static shape. The shape — a native's
//! path and name — stays `const`, so the runtime can name a failure without a
//! program.
//!
//! The row is also the one declaration of a native's argument order and types:
//! lowering turns each parameter type into a `NativeArgumentKind`, and the
//! runtime's single decoder uses that list to check the arity and shape of a
//! call before a handler runs. A handler therefore reads typed arguments by
//! their declared position and never pops a stack.
//!
//! To add a native, add one entry to `native_registry!` and one arm to
//! `Runtime::call_native`. There is no other way: the macro generates the
//! variant, the shape, and the typed row together, the checker reads the row,
//! the runtime's handler match is exhaustive, and the runtime decodes exactly
//! the parameters the row declares.

use crate::compiler::STD_TREE;
use crate::compiler::graph::{Declaration, RuntimeShape, Type};

/// How the runtime decodes one native argument: the parameter type the row
/// declares, reduced to the runtime shape the decoder enforces. A handler then
/// reads the argument through the accessor this kind names.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) enum NativeArgumentKind {
    /// Text, passed as a shared immutable text.
    Text,
    /// An integer.
    Int,
    /// A boolean.
    Bool,
    /// The runtime list, passed as a shared Vec of values.
    List,
    /// The runtime table, passed as a shared table of values.
    Table,
    /// A declared record, an enum variant, or a map.
    Object,
    /// A function value.
    Function,
    /// A type parameter, such as the list's element: any value fits.
    Any,
}

/// The runtime shape of one parameter type a row declares. This reads the
/// type's runtime shape, which the group facts declare: a scalar is a machine
/// word of its own kind, a declared type is a record, the runtime list, or a
/// variant, a function is a function value, and a type parameter is the
/// caller's type, so it admits any value.
pub(crate) fn argument_kind(ty: &Type, declarations: &[Declaration]) -> NativeArgumentKind {
    match ty.runtime_shape(declarations) {
        RuntimeShape::Text => NativeArgumentKind::Text,
        RuntimeShape::Int => NativeArgumentKind::Int,
        RuntimeShape::Bool => NativeArgumentKind::Bool,
        RuntimeShape::List => NativeArgumentKind::List,
        RuntimeShape::Table => NativeArgumentKind::Table,
        RuntimeShape::Record | RuntimeShape::Variant => NativeArgumentKind::Object,
        RuntimeShape::Function => NativeArgumentKind::Function,
        RuntimeShape::Parameter => NativeArgumentKind::Any,
        RuntimeShape::Nothing => unreachable!("a native's parameters are types, never the absence"),
    }
}

/// One native's declared parameter shapes, in order. Lowering writes this list
/// into the program image, so the runtime decodes a call from the row the
/// checker read instead of from a hand-written stack discipline.
#[derive(Clone, Debug, serde::Serialize, serde::Deserialize)]
pub(crate) struct NativeSignature {
    pub(crate) native: Native,
    pub(crate) parameters: Vec<NativeArgumentKind>,
}

/// A type a native row names.
#[derive(Debug)]
pub(crate) enum NativeType {
    /// A type the row writes directly, such as a scalar.
    Direct(Type),
    /// A declared type named by its absolute path, with its type arguments,
    /// resolved through the ordinary type resolver in the native's namespace.
    Path {
        path: &'static [&'static str],
        arguments: Vec<NativeType>,
    },
    /// A parameter of the container the row names, spelled by that container's
    /// own declaration.
    Parameter(NativeParameter),
}

/// One parameter of a runtime container, named so a row can refer to it
/// without repeating the spelling the declaration chose.
#[derive(Clone, Copy, Debug)]
pub(crate) enum NativeParameter {
    /// The list's element.
    Element,
    /// The table's key.
    Key,
    /// The table's value.
    Value,
}

impl From<Type> for NativeType {
    fn from(ty: Type) -> Self {
        NativeType::Direct(ty)
    }
}

/// The runtime list applied to `element`, named by the path of its declaration.
fn list_of(element: impl Into<NativeType>) -> NativeType {
    NativeType::Path {
        path: &[STD_TREE, "dynamic", "Vec"],
        arguments: vec![element.into()],
    }
}

/// The runtime table applied to `key` and `value`, named by the path of its
/// declaration.
fn hash_map_of(key: impl Into<NativeType>, value: impl Into<NativeType>) -> NativeType {
    NativeType::Path {
        path: &[STD_TREE, "dynamic", "HashMap"],
        arguments: vec![key.into(), value.into()],
    }
}

/// The standard library's `Option` applied to `element`, named by the path of
/// its declaration.
fn option_of(element: impl Into<NativeType>) -> NativeType {
    NativeType::Path {
        path: &[STD_TREE, "Option"],
        arguments: vec![element.into()],
    }
}

/// The runtime list's element parameter, read from the list declaration when
/// the row is resolved. A call binds it through the receiver, the same `unify`
/// every declaration uses.
fn element() -> NativeType {
    NativeType::Parameter(NativeParameter::Element)
}

/// The runtime table's key parameter.
fn key() -> NativeType {
    NativeType::Parameter(NativeParameter::Key)
}

/// The runtime table's value parameter.
fn value() -> NativeType {
    NativeType::Parameter(NativeParameter::Value)
}

/// One native row: its path, name, parameter types, and result type. A native
/// with no result type produces no value; its calls are statements, exactly as
/// a kiru function with no return type. Its types name what any other
/// declaration names, by path or by the list's own parameter.
#[derive(Debug)]
pub(crate) struct NativeRow {
    pub(crate) path: &'static [&'static str],
    pub(crate) name: &'static str,
    pub(crate) parameters: Vec<NativeType>,
    pub(crate) returns: Option<NativeType>,
}

/// The part of a row that does not depend on a resolved type: where the native
/// lives and what it is called. It stays `const`, so the runtime can name a
/// failure without a program.
#[derive(Clone, Copy, Debug)]
pub(crate) struct NativeShape {
    pub(crate) path: &'static [&'static str],
    pub(crate) name: &'static str,
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

/// Declare every native once: the enum variant, its static shape, its typed
/// row, and the complete lists. Because the runtime's handler match is
/// exhaustive over the variants, a native cannot exist without a row and a
/// handler.
///
/// A row names its namespace under the library tree, so the tree's name is
/// written once; its types are symbolic: a declared type's absolute path,
/// `list_of`, `option_of`, and `element`, or a direct type. Resolution happens
/// once, in the stage that registers the natives.
macro_rules! native_registry {
    ($( $variant:ident { [$($segment:literal),* $(,)?], $name:literal, [$($parameter:expr),* $(,)?] $(-> $returns:expr)? } ),* $(,)?) => {
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

        /// Every native's path and name. This needs no resolved type, so it
        /// stays `const` and serves the runtime's failure text.
        pub(crate) const NATIVE_SHAPES: &[(Native, NativeShape)] = &[
            $( (
                Native::$variant,
                NativeShape {
                    path: &[STD_TREE, $($segment),*],
                    name: $name,
                },
            ) ),*
        ];

        /// Every native with its typed row, built once per program because a
        /// `Type` is not `const`-constructible. A row's symbolic types are
        /// resolved by the caller that registers the natives.
        pub(crate) fn native_rows() -> Vec<(Native, NativeRow)> {
            vec![
                $( (
                    Native::$variant,
                    NativeRow {
                        path: &[STD_TREE, $($segment),*],
                        name: $name,
                        parameters: vec![ $( NativeType::from($parameter) ),* ],
                        returns: optional!($( NativeType::from($returns) )?),
                    },
                ) ),*
            ]
        }
    };
}

native_registry! {
    // The pure vec operations stay in `std::dynamic`.
    VecNew { ["dynamic"], "new", [] -> list_of(element()) },
    VecPush { ["dynamic"], "push", [list_of(element()), element()] -> list_of(element()) },
    VecReplace { ["dynamic"], "replace", [list_of(element()), Type::Int, element()] -> list_of(element()) },
    VecAt { ["dynamic"], "at", [list_of(element()), Type::Int] -> element() },
    VecLen { ["dynamic"], "len", [list_of(element())] -> Type::Int },
    // The pure hash map operations stay in `std::dynamic` beside the vec's.
    // `find` answers `Option` so `get`, `get_or`, and `contains` are kiru over
    // one lookup; every other operation is the one the runtime can do without
    // walking a structure in the virtual machine.
    HashMapNew { ["dynamic"], "new", [] -> hash_map_of(key(), value()) },
    HashMapInsert { ["dynamic"], "insert", [hash_map_of(key(), value()), key(), value()] -> hash_map_of(key(), value()) },
    HashMapFind { ["dynamic"], "find", [hash_map_of(key(), value()), key()] -> option_of(value()) },
    HashMapRemove { ["dynamic"], "remove", [hash_map_of(key(), value()), key()] -> hash_map_of(key(), value()) },
    HashMapLen { ["dynamic"], "len", [hash_map_of(key(), value())] -> Type::Int },
    HashMapKeys { ["dynamic"], "keys", [hash_map_of(key(), value())] -> list_of(key()) },
    HashMapValues { ["dynamic"], "values", [hash_map_of(key(), value())] -> list_of(value()) },
    // Reading and writing a path are file operations, not syscalls, so they
    // live beside the rest of `std::fs`.
    Read { ["fs"], "read", [Type::Text] -> Type::Text },
    WriteFile { ["fs"], "write", [Type::Text, Type::Text] },
    // The pure byte kernels operate on text and touch no OS, so they stay
    // beside their kind under their libc names.
    Strlen { ["text"], "strlen", [Type::Text] -> Type::Int },
    Memchr { ["text"], "memchr", [Type::Text, Type::Text] -> option_of(Type::Int) },
    Substr { ["text"], "substr", [Type::Text, Type::Int, Type::Int] -> Type::Text },
    ByteAt { ["text"], "byte_at", [Type::Text, Type::Int] -> Type::Int },
    // Every native that touches the OS lives in `std::syscalls`, under its
    // syscall-like name.
    Write { ["syscalls"], "write", [Type::Int, Type::Text] },
    Unlink { ["syscalls"], "unlink", [Type::Text] },
    Access { ["syscalls"], "access", [Type::Text] -> Type::Bool },
    Mkstemp { ["syscalls"], "mkstemp", [Type::Text] -> Type::Text },
    Readdir { ["syscalls"], "readdir", [Type::Text] -> list_of(Type::Text) },
    Getenv { ["syscalls"], "getenv", [Type::Text] -> option_of(Type::Text) },
    Getcwd { ["syscalls"], "getcwd", [] -> Type::Text },
    Isatty { ["syscalls"], "isatty", [Type::Int] -> Type::Bool },
    ReadStdin { ["syscalls"], "read_stdin", [] -> Type::Text },
    PosixSpawn { ["syscalls"], "posix_spawn", [list_of(Type::Text)] -> Type::Int },
    Waitpid { ["syscalls"], "waitpid", [Type::Int] -> Type::Int },
    Killpg { ["syscalls"], "killpg", [Type::Int, Type::Int] },
    Nanosleep { ["syscalls"], "nanosleep", [Type::Int] },
}

/// The shape of a native: its path, name, and parameter count.
pub(crate) fn native_shape(native: Native) -> &'static NativeShape {
    NATIVE_SHAPES
        .iter()
        .find(|(id, _)| *id == native)
        .map(|(_, shape)| shape)
        .expect("every native has exactly one shape")
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
    fn every_native_has_exactly_one_shape() {
        // A name is unique in a namespace unless the functions bind to
        // different types: `Vec.len` and `HashMap.len` share the spelling
        // `len`, and the receiver's head selects between them. The row table is
        // keyed by the native itself, so one variant never gets two rows.
        for (index, (native, row)) in NATIVE_SHAPES.iter().enumerate() {
            for (other, other_row) in NATIVE_SHAPES.iter().skip(index + 1) {
                assert_ne!(
                    native, other,
                    "natives `{}` and `{}` share one shape row",
                    row.name, other_row.name
                );
            }
        }
    }

    #[test]
    fn natives_live_under_the_builtin_namespace() {
        for (native, row) in NATIVE_SHAPES {
            assert!(
                row.path.first().copied() == Some(STD_TREE),
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
