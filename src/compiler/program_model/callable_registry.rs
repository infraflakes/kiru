//! The native call registry: every builtin the language ships.
//!
//! A row carries its namespace path, its name, the kind of each parameter, and
//! its result kind. The resolution stage registers natives from these rows as
//! ordinary callables with parameter nodes, so the checker treats a native and
//! a user function through one path. The runtime dispatches on the `Native`
//! id; its handler match is exhaustive, so an id without a handler does not
//! compile.

use super::value_kinds::Kind;

/// The entry function name the runtime calls.
pub(crate) const ENTRY_FUNCTION: &str = "main";

/// The namespace the runtime's builtins live in.
pub(crate) const BUILTIN_NAMESPACE: &[&str] = &["std"];

/// A native provided by the runtime.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) enum Native {
    Run,
    Quote,
}

impl Native {
    /// Every native, for registry completeness checks.
    #[cfg(test)]
    pub(crate) fn all() -> &'static [Native] {
        &[Native::Run, Native::Quote]
    }
}

/// One native row: its path, name, parameter kinds, and result kind.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Row {
    pub(crate) path: &'static [&'static str],
    pub(crate) name: &'static str,
    pub(crate) parameters: &'static [Kind],
    pub(crate) returns: Kind,
}

/// Every native the runtime provides, with its row.
pub(crate) const NATIVE_ROWS: &[(Native, Row)] = &[
    (
        Native::Run,
        Row {
            path: BUILTIN_NAMESPACE,
            name: "run",
            parameters: &[Kind::Text],
            returns: Kind::Text,
        },
    ),
    (
        Native::Quote,
        Row {
            path: BUILTIN_NAMESPACE,
            name: "quote",
            parameters: &[Kind::Text],
            returns: Kind::Text,
        },
    ),
];

/// The row of a native.
pub(crate) fn native_row(native: Native) -> &'static Row {
    NATIVE_ROWS
        .iter()
        .find(|(id, _)| *id == native)
        .map(|(_, row)| row)
        .expect("every native has exactly one row")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_native_id_has_exactly_one_row() {
        for native in Native::all() {
            let rows = NATIVE_ROWS.iter().filter(|(id, _)| id == native).count();
            assert_eq!(rows, 1, "{native:?} must have exactly one row");
        }
        assert_eq!(NATIVE_ROWS.len(), Native::all().len());
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
