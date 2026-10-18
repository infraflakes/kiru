//! The standard library's files, compiled into every program.
//!
//! This is a content table, not a graph: it mirrors `std/**` exactly, and each
//! file's directory under `std/` is its namespace. A file added to the tree
//! must be added here.

/// The standard library's files, compiled into every program.
pub(crate) const EMBEDDED: &[(&str, &str)] = &[
    ("<std>/std.kiru", include_str!("../../../../std/std.kiru")),
    (
        "<std>/cli/cli.kiru",
        include_str!("../../../../std/cli/cli.kiru"),
    ),
    (
        "<std>/dynamic/map.kiru",
        include_str!("../../../../std/dynamic/map.kiru"),
    ),
    (
        "<std>/dynamic/set.kiru",
        include_str!("../../../../std/dynamic/set.kiru"),
    ),
    (
        "<std>/dynamic/vec.kiru",
        include_str!("../../../../std/dynamic/vec.kiru"),
    ),
    (
        "<std>/fs/fs.kiru",
        include_str!("../../../../std/fs/fs.kiru"),
    ),
    (
        "<std>/fs/path.kiru",
        include_str!("../../../../std/fs/path.kiru"),
    ),
    (
        "<std>/process/env.kiru",
        include_str!("../../../../std/process/env.kiru"),
    ),
    (
        "<std>/process/process.kiru",
        include_str!("../../../../std/process/process.kiru"),
    ),
    (
        "<std>/process/time.kiru",
        include_str!("../../../../std/process/time.kiru"),
    ),
    (
        "<std>/text/text.kiru",
        include_str!("../../../../std/text/text.kiru"),
    ),
];
