//! The native call registry: every builtin the language ships.
//!
//! A row carries its namespace path, its name, the kind of each parameter, and
//! its result kind. The resolution stage registers natives from these rows as
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

use crate::model::Kind;

/// The namespace the runtime's builtins live in.
pub(crate) const BUILTIN_NAMESPACE: &[&str] = &["std"];

/// One native row: its path, name, parameter kinds, and result kind.
#[derive(Clone, Copy, Debug)]
pub(crate) struct NativeRow {
    pub(crate) path: &'static [&'static str],
    pub(crate) name: &'static str,
    pub(crate) parameters: &'static [Kind],
    pub(crate) returns: Kind,
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
    Spawn { ["std", "process"], "spawn", [Kind::Record, Kind::List] -> Kind::Text },
    WaitFor { ["std", "process"], "wait_for", [Kind::Text] -> Kind::Text },
    Signal { ["std", "process"], "signal", [Kind::Text, Kind::Text] -> Kind::Nothing },
    Write { ["std", "io"], "write", [Kind::Text, Kind::Text] -> Kind::Nothing },
    IsTerminal { ["std", "io"], "is_terminal", [Kind::Text] -> Kind::Text },
    ReadStdin { ["std", "io"], "read_stdin", [] -> Kind::Text },
    Read { ["std", "fs"], "read", [Kind::Text] -> Kind::Text },
    WriteFile { ["std", "fs"], "write", [Kind::Text, Kind::Text] -> Kind::Nothing },
    Append { ["std", "fs"], "append", [Kind::Text, Kind::Text] -> Kind::Nothing },
    Remove { ["std", "fs"], "remove", [Kind::Text] -> Kind::Nothing },
    Exists { ["std", "fs"], "exists", [Kind::Text] -> Kind::Text },
    TempPath { ["std", "fs"], "temp_file", [] -> Kind::Text },
    Glob { ["std", "fs"], "glob", [Kind::Text] -> Kind::List },
    Length { ["std", "text"], "len", [Kind::Text] -> Kind::Text },
    Slice { ["std", "text"], "slice", [Kind::Text, Kind::Text, Kind::Text] -> Kind::Text },
    Find { ["std", "text"], "find", [Kind::Text, Kind::Text] -> Kind::Text },
    Trim { ["std", "text"], "trim", [Kind::Text] -> Kind::Text },
    Split { ["std", "text"], "split", [Kind::Text, Kind::Text] -> Kind::List },
    Lines { ["std", "text"], "lines", [Kind::Text] -> Kind::List },
    Sleep { ["std", "time"], "sleep", [Kind::Text] -> Kind::Nothing },
    ListLength { ["std", "lists"], "len", [Kind::List] -> Kind::Text },
    ListGet { ["std", "lists"], "get", [Kind::List, Kind::Text] -> Kind::Text },
    ListAppend { ["std", "lists"], "append", [Kind::List, Kind::Text] -> Kind::List },
    ListReverse { ["std", "lists"], "reverse", [Kind::List] -> Kind::List },
    ListSort { ["std", "lists"], "sort", [Kind::List] -> Kind::List },
    EnvVar { ["std", "env"], "var", [Kind::Text] -> Kind::Text },
    CurrentDir { ["std", "env"], "current_dir", [] -> Kind::Text },
    PathBase { ["std", "path"], "base", [Kind::Text] -> Kind::Text },
    PathDir { ["std", "path"], "dir", [Kind::Text] -> Kind::Text },
    PathExt { ["std", "path"], "ext", [Kind::Text] -> Kind::Text },
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
