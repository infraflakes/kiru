//! The call registry: every native and method the language ships.
//!
//! A row carries its namespace path, its name, how it accepts each argument,
//! and its result kind. The linker registers natives from these rows, the
//! checker derives arity and argument compatibility from them, and the
//! interpreter dispatches on the row id. The row ids are the `Native` and
//! `Method` enums; their handler matches are exhaustive, so an id without a
//! handler does not compile.

use super::kinds::{Kind, Usage};

/// The entry function name the runtime calls.
pub(crate) const ENTRY_FUNCTION: &str = "main";

/// The namespace the runtime's builtins live in.
pub(crate) const BUILTIN_NAMESPACE: &[&str] = &["std"];

/// A native provided by the runtime.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) enum Native {
    Command,
    Async,
    Wait,
    Panic,
}

impl Native {
    /// Every native, for registry completeness checks.
    #[cfg(test)]
    pub(crate) fn all() -> &'static [Native] {
        &[Native::Command, Native::Async, Native::Wait, Native::Panic]
    }
}

/// A command method: a builder or a terminal.
#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub(crate) enum Method {
    In,
    Direnv,
    Env,
    Shell,
    Timeout,
    Stream,
    Out,
    Code,
}

impl Method {
    /// Every method, for registry completeness checks.
    #[cfg(test)]
    pub(crate) fn all() -> &'static [Method] {
        &[
            Method::In,
            Method::Direnv,
            Method::Env,
            Method::Shell,
            Method::Timeout,
            Method::Stream,
            Method::Out,
            Method::Code,
        ]
    }

    /// The method reached by a name, if any.
    pub(crate) fn from_name(name: &str) -> Option<Method> {
        METHOD_ROWS
            .iter()
            .find(|(_, row)| row.name == name)
            .map(|(method, _)| *method)
    }

    /// The name the method is reached by.
    pub(crate) fn name(self) -> &'static str {
        method_row(self).name
    }
}

/// How one call accepts an argument.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Accept {
    /// The argument's kind must fit this usage; only a single-kind
    /// requirement occurs, because every row asks for text or record.
    Usage(Usage),
    /// The argument must be an invocation: a call to a function or native, or
    /// a command method chain.
    Invocation,
}

/// One callable row. Methods live outside any namespace and carry an empty
/// path.
#[derive(Clone, Copy, Debug)]
pub(crate) struct Row {
    pub(crate) path: &'static [&'static str],
    pub(crate) name: &'static str,
    pub(crate) accepts: &'static [Accept],
    pub(crate) returns: Kind,
}

impl Row {
    /// Whether the call finishes a command chain: every method except the
    /// terminals returns a command, and a terminal runs it and binds text.
    pub(crate) fn is_terminal(&self) -> bool {
        self.returns != Kind::Command
    }
}

/// Every native the runtime provides, with its row.
pub(crate) const NATIVE_ROWS: &[(Native, Row)] = &[
    (
        Native::Command,
        Row {
            path: &["std"],
            name: "command",
            accepts: &[Accept::Usage(Usage::TEXT)],
            returns: Kind::Command,
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
    (
        Native::Panic,
        Row {
            path: &["std"],
            name: "panic",
            accepts: &[],
            returns: Kind::Never,
        },
    ),
];

/// Every command method, in the order the language documents them.
pub(crate) const METHOD_ROWS: &[(Method, Row)] = &[
    (
        Method::In,
        Row {
            path: &[],
            name: "in",
            accepts: &[Accept::Usage(Usage::TEXT)],
            returns: Kind::Command,
        },
    ),
    (
        Method::Direnv,
        Row {
            path: &[],
            name: "direnv",
            accepts: &[],
            returns: Kind::Command,
        },
    ),
    (
        Method::Env,
        Row {
            path: &[],
            name: "env",
            accepts: &[Accept::Usage(Usage::RECORD)],
            returns: Kind::Command,
        },
    ),
    (
        Method::Shell,
        Row {
            path: &[],
            name: "shell",
            accepts: &[Accept::Usage(Usage::TEXT)],
            returns: Kind::Command,
        },
    ),
    (
        Method::Timeout,
        Row {
            path: &[],
            name: "timeout",
            accepts: &[Accept::Usage(Usage::TEXT)],
            returns: Kind::Command,
        },
    ),
    (
        Method::Stream,
        Row {
            path: &[],
            name: "stream",
            accepts: &[],
            returns: Kind::Command,
        },
    ),
    (
        Method::Out,
        Row {
            path: &[],
            name: "out",
            accepts: &[],
            returns: Kind::Text,
        },
    ),
    (
        Method::Code,
        Row {
            path: &[],
            name: "code",
            accepts: &[],
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

/// The row of a command method.
pub(crate) fn method_row(method: Method) -> &'static Row {
    METHOD_ROWS
        .iter()
        .find(|(id, _)| *id == method)
        .map(|(_, row)| row)
        .expect("every method has exactly one row")
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
    fn every_method_id_has_exactly_one_row() {
        for method in Method::all() {
            let rows = METHOD_ROWS.iter().filter(|(id, _)| id == method).count();
            assert_eq!(rows, 1, "{method:?} must have exactly one row");
        }
        assert_eq!(METHOD_ROWS.len(), Method::all().len());
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
    fn method_names_are_unique() {
        for (index, (_, row)) in METHOD_ROWS.iter().enumerate() {
            for (_, other) in METHOD_ROWS.iter().skip(index + 1) {
                assert_ne!(row.name, other.name, "two methods share a name");
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
        let rows = NATIVE_ROWS
            .iter()
            .map(|(_, row)| row)
            .chain(METHOD_ROWS.iter().map(|(_, row)| row));
        for row in rows {
            for accept in row.accepts {
                if let Accept::Usage(usage) = accept {
                    assert!(
                        matches!(
                            *usage,
                            Usage::TEXT
                                | Usage::RECORD
                                | Usage::COMMAND
                                | Usage::NOTHING
                                | Usage::NEVER
                        ),
                        "row `{}` asks for a group",
                        row.name
                    );
                }
            }
        }
    }
}
