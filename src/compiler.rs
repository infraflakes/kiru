//! # Compiler Pipeline
//!
//! A single-pass eager pipeline. Parsing is a separate front-end that feeds the
//! compiler; runtime is a separate back-end that consumes its output.
//!
//! The compiler's output is a [`crate::plan::Plan`] — a fully lowered
//! configuration where every variable reference has been substituted and
//! function bodies contain only pure [`crate::plan::PlanStmt`]s (no `Expr` or
//! `VarDecl` nodes remain).  Structural metadata like project fields and `Run`
//! declarations are collected and passed through without transformation — only
//! function bodies undergo full lowering. The runner, sync driver, and CLI
//! consume `crate::plan` and never reach back into this module.
//!
//! Items are walked in source order, one pass, no deferred resolution:
//!
//! - **Global `var`/`var shell`** — resolved immediately at their declaration
//!   point, so a later `import` path or global can read `global::name`.
//! - **Global `fn`** — stored as an AST template (no resolution, only scope
//!   validation). `self::` is left symbolic.
//! - **`pr` block** — fields are resolved immediately (only `global::`
//!   references), then body statements are processed eagerly in source order:
//!   `var`/`var shell` are resolved at their declaration, `use fn` clones the
//!   template, rewrites `self::`, validates, and resolves the function body
//!   immediately (with compile-time case arm matching). Case arms are matched at
//!   compile time so `var shell` in unreachable arms never executes.
//! - **`run` block** — function refs are validated against accumulated projects.

pub(crate) mod compile;
pub(crate) mod error;
pub(crate) mod fnstmt;
pub(crate) mod namespaces;
pub(crate) mod scope;
pub(crate) mod validation;

/// Run the full pipeline: parse, merge, resolve includes, validate, and resolve.
pub use compile::compile_and_resolve;
/// Lightweight compilation: parse, resolve vars, resolve project fields.
/// Skips validation and function body lowering.  Used by `kiru sync`.
pub use compile::parse_projects_metadata;
pub use error::CompileError;

#[cfg(test)]
pub(crate) mod test_support;
