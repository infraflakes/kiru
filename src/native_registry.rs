//! The native call registry: every builtin the language ships.
//!
//! A row carries its namespace path, its name, the type of each parameter, and
//! its result type. The resolution stage registers natives from these rows as
//! ordinary callables with parameter nodes, so the checker treats a native and
//! a user function through one path. The runtime dispatches on the `Native`
//! id; its handler match is exhaustive, so an id without a handler does not
//! compile.
//!
//! To add a native, add one entry to `native_registry!` and one arm to
//! `Runtime::call_native`. There is no other way: the macro generates the
//! variant and its row together, the checker reads the row, the runtime's
//! handler match is exhaustive, and the runtime checks the handler consumes
//! exactly the declared arity.

use crate::types::Type;

/// The namespace the runtime's builtins live in.
pub(crate) const BUILTIN_NAMESPACE: &[&str] = &["std"];

/// One native row: its path, name, parameter types, and result type. A native
/// that returns no value has result type `Void`.
#[derive(Clone, Copy, Debug)]
pub(crate) struct NativeRow {
    pub(crate) path: &'static [&'static str],
    pub(crate) name: &'static str,
    pub(crate) parameters: &'static [Type],
    pub(crate) returns: Type,
}

/// Declare every native once: the enum variant, its row, and the complete
/// list. Because the runtime's handler match is exhaustive over the variants,
/// a native cannot exist without a row and a handler.
macro_rules! native_registry {
    ($( $variant:ident { [$($segment:literal),* $(,)?], $name:literal, [$($parameter:expr),* $(,)?] -> $returns:expr } ),* $(,)?) => {
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

        /// Every native the runtime provides, with its row.
        pub(crate) const NATIVE_ROWS: &[(Native, NativeRow)] = &[
            $( (
                Native::$variant,
                NativeRow {
                    path: &[$($segment),*],
                    name: $name,
                    parameters: &[$($parameter),*],
                    returns: $returns,
                },
            ) ),*
        ];
    };
}

native_registry! {
    Spawn { ["std", "process"], "spawn", [Type::Record, Type::List] -> Type::Text },
    WaitFor { ["std", "process"], "wait_for", [Type::Text] -> Type::Text },
    Signal { ["std", "process"], "signal", [Type::Text, Type::Text] -> Type::Void },
    Write { ["std", "io"], "write", [Type::Text, Type::Text] -> Type::Void },
    IsTerminal { ["std", "io"], "is_terminal", [Type::Text] -> Type::Text },
    ReadStdin { ["std", "io"], "read_stdin", [] -> Type::Text },
    Read { ["std", "fs"], "read", [Type::Text] -> Type::Text },
    WriteFile { ["std", "fs"], "write", [Type::Text, Type::Text] -> Type::Void },
    Remove { ["std", "fs"], "remove", [Type::Text] -> Type::Void },
    Exists { ["std", "fs"], "exists", [Type::Text] -> Type::Text },
    TempPath { ["std", "fs"], "temp_file", [] -> Type::Text },
    Glob { ["std", "fs"], "glob", [Type::Text] -> Type::List },
    Length { ["std", "text"], "len", [Type::Text] -> Type::Text },
    Slice { ["std", "text"], "slice", [Type::Text, Type::Text, Type::Text] -> Type::Text },
    Find { ["std", "text"], "find", [Type::Text, Type::Text] -> Type::Text },
    Trim { ["std", "text"], "trim", [Type::Text] -> Type::Text },
    Split { ["std"], "split", [Type::Text, Type::Text] -> Type::List },
    Sleep { ["std", "time"], "sleep", [Type::Text] -> Type::Void },
    ListLength { ["std", "lists"], "len", [Type::List] -> Type::Text },
    ListGet { ["std"], "get", [Type::List, Type::Text] -> Type::Text },
    ListAppend { ["std"], "append", [Type::List, Type::Text] -> Type::List },
    ListPrepend { ["std", "lists"], "prepend", [Type::List, Type::Text] -> Type::List },
    ListSort { ["std", "lists"], "sort", [Type::List] -> Type::List },
    EnvVar { ["std", "env"], "var", [Type::Text] -> Type::Text },
    CurrentDir { ["std", "env"], "current_dir", [] -> Type::Text },
}

/// The row of a native.
pub(crate) fn native_row(native: Native) -> &'static NativeRow {
    NATIVE_ROWS
        .iter()
        .find(|(id, _)| *id == native)
        .map(|(_, row)| row)
        .expect("every native has exactly one row")
}

/// The number of parameters a native declares. The runtime checks that a
/// handler consumes exactly this many arguments, so a handler cannot drift
/// from its row.
pub(crate) fn native_arity(native: Native) -> usize {
    native_row(native).parameters.len()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_native_id_has_exactly_one_row() {
        for native in Native::ALL {
            let rows = NATIVE_ROWS.iter().filter(|(id, _)| id == native).count();
            assert_eq!(rows, 1, "{native:?} must have exactly one row");
        }
        assert_eq!(NATIVE_ROWS.len(), Native::ALL.len());
    }

    #[test]
    fn native_paths_and_names_are_unique() {
        for (index, (_, row)) in NATIVE_ROWS.iter().enumerate() {
            for (_, other) in NATIVE_ROWS.iter().skip(index + 1) {
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
        for (native, row) in NATIVE_ROWS {
            assert!(
                row.path.first().copied() == Some(BUILTIN_NAMESPACE[0]),
                "native {native:?} is outside the builtin namespace"
            );
        }
    }
}
