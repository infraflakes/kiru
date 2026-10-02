//! The call registry: every native the language ships.
//!
//! A row carries its namespace path, its name, how it accepts each argument,
//! and its result kind. The linker registers natives from these rows, the
//! checker derives arity and argument compatibility from them, and the
//! interpreter dispatches on the row id. The row ids are the `Native` enum;
//! its handler match is exhaustive, so an id without a handler does not
//! compile.

use super::kinds::{Kind, Usage};

/// The entry function name the runtime calls.
pub(crate) const ENTRY_FUNCTION: &str = "main";

/// The namespace the runtime's builtins live in.
pub(crate) const BUILTIN_NAMESPACE: &[&str] = &["std"];

/// A native provided by the runtime.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) enum Native {
    Run,
    Async,
    Wait,
}

impl Native {
    /// Every native, for registry completeness checks.
    #[cfg(test)]
    pub(crate) fn all() -> &'static [Native] {
        &[Native::Run, Native::Async, Native::Wait]
    }
}

/// How one call accepts an argument.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Accept {
    /// The argument's kind must fit this usage; only a single-kind
    /// requirement occurs, because every row asks for text or record.
    Usage(Usage),
    /// The argument must be an invocation: a call to a function or native.
    Invocation,
}

/// One callable row.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Row {
    pub(crate) path: &'static [&'static str],
    pub(crate) name: &'static str,
    pub(crate) accepts: &'static [Accept],
    pub(crate) returns: Kind,
}

/// Every native the runtime provides, with its row.
pub(crate) const NATIVE_ROWS: &[(Native, Row)] = &[
    (
        Native::Run,
        Row {
            path: &["std"],
            name: "run",
            accepts: &[Accept::Usage(Usage::TEXT)],
            returns: Kind::Record,
        },
    ),
    (
        Native::Async,
        Row {
            path: &["std"],
            name: "async",
            accepts: &[Accept::Invocation],
            returns: Kind::Nothing,
        },
    ),
    (
        Native::Wait,
        Row {
            path: &["std"],
            name: "wait",
            accepts: &[],
            returns: Kind::Nothing,
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

    /// A row requirement names one kind so its diagnostic can name that kind;
    /// a group requirement would leave the caller no single name to report.
    #[test]
    fn every_usage_requirement_is_one_concrete_kind() {
        let rows = NATIVE_ROWS.iter().map(|(_, row)| row);
        for row in rows {
            for accept in row.accepts {
                if let Accept::Usage(usage) = accept {
                    assert!(
                        matches!(*usage, Usage::TEXT | Usage::RECORD | Usage::NOTHING),
                        "row `{}` asks for a group",
                        row.name
                    );
                }
            }
        }
    }
}
